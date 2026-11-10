#!/usr/bin/env python3
"""Records the narration with an open-source text-to-speech voice, running locally.

    python3 tools/narrate.py cues.json MODEL_DIR OUT_DIR

MODEL_DIR is the sherpa-onnx "kokoro-multi-lang-v1_0" bundle (Kokoro-82M, Apache-2.0):
    https://github.com/k2-fsa/sherpa-onnx/releases/tag/tts-models
Writes OUT_DIR/<id>.wav (48 kHz mono, one file per line) and OUT_DIR/vo.json with each
line's start time, text and measured duration, which the captions and the mix both read.
"""
import json
import sys

import numpy as np
import sherpa_onnx
from scipy import signal
from scipy.io import wavfile

VOICE = 27     # bm_lewis: a deep British male voice
SPEED = 0.86   # slower than conversational; this is a trailer
OUT_SR = 48000

cues = json.load(open(sys.argv[1]))
model = sys.argv[2]
out = sys.argv[3]

cfg = sherpa_onnx.OfflineTtsConfig(
    model=sherpa_onnx.OfflineTtsModelConfig(
        kokoro=sherpa_onnx.OfflineTtsKokoroModelConfig(
            model=f"{model}/model.onnx", voices=f"{model}/voices.bin", tokens=f"{model}/tokens.txt",
            lexicon=f"{model}/lexicon-gb-en.txt", data_dir=f"{model}/espeak-ng-data", dict_dir=f"{model}/dict", lang="en-us"),
        num_threads=2, provider="cpu"))
tts = sherpa_onnx.OfflineTts(cfg)


def trim(x, sr, thresh=0.012, pad=0.04):
    idx = np.where(np.abs(x) > thresh)[0]
    if len(idx) == 0:
        return x
    a, b = max(0, idx[0] - int(pad * sr)), min(len(x), idx[-1] + int(pad * sr))
    return x[a:b]


def shape(x):
    """A touch of weight and presence, a gentle compressor, a short fade."""
    sos = signal.butter(2, 70, "high", fs=OUT_SR, output="sos")
    x = signal.sosfilt(sos, x)
    low = signal.sosfilt(signal.butter(2, 220, "low", fs=OUT_SR, output="sos"), x)
    pres = signal.sosfilt(signal.butter(2, [2200, 4200], "band", fs=OUT_SR, output="sos"), x)
    x = x + 0.45 * low + 0.22 * pres
    # compressor: smooth envelope, 3:1 above a threshold
    env = np.sqrt(signal.sosfilt(signal.butter(1, 12, "low", fs=OUT_SR, output="sos"), x ** 2) + 1e-9)
    thr = 0.18
    gain = np.where(env > thr, (thr + (env - thr) / 3) / env, 1.0)
    x = x * gain
    x *= 0.6 / (np.sqrt(np.mean(x ** 2)) * 4 + 1e-9) * 0.25
    n = int(0.03 * OUT_SR)
    x[:n] *= np.linspace(0, 1, n)
    x[-n:] *= np.linspace(1, 0, n)
    return x


timing = []
for line in cues["narration"]:
    a = tts.generate(line["text"], sid=VOICE, speed=SPEED)
    x = np.array(a.samples, dtype=np.float32)
    x = trim(x, a.sample_rate)
    x = signal.resample_poly(x, OUT_SR, a.sample_rate)
    x = shape(x)
    peak = np.max(np.abs(x))
    x = x / peak * 0.8
    wavfile.write(f"{out}/{line['id']}.wav", OUT_SR, (x * 32767).astype(np.int16))
    d = len(x) / OUT_SR
    timing.append({"id": line["id"], "at": line["at"], "dur": round(d, 3), "text": line["text"]})
    print(f"{line['id']}  {line['at']:6.2f} -> {line['at'] + d:6.2f}  ({d:4.2f}s)  {line['text']}")

json.dump(timing, open(f"{out}/vo.json", "w"), indent=1)

# the lines must not talk over each other
for p, q in zip(timing, timing[1:]):
    if p["at"] + p["dur"] > q["at"] + 0.05:
        print(f"WARNING: {p['id']} runs into {q['id']} by {p['at'] + p['dur'] - q['at']:.2f}s")
