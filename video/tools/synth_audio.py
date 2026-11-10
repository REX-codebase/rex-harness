#!/usr/bin/env python3
"""Synthesizes the film's score and sound design, with no samples and no external assets.

    python3 tools/synth_audio.py cues.json out.wav

Everything is generated from the cue sheet (tools/print-cues.mjs), so the audio is locked to the
same timeline as the picture: the click lands on the frame the cursor presses, the stop lands on
the frame the rings freeze.  Needs numpy and scipy.
"""
import json
import sys

import numpy as np
from scipy import signal
from scipy.io import wavfile

SR = 48000
rng = np.random.default_rng(7)

cues = json.load(open(sys.argv[1]))
OUT = sys.argv[2]
DUR = cues["duration"] + 2.0  # render a 2 s tail; the mux trims it
N = int(DUR * SR)
S = cues["shots"]

# ── note table ──────────────────────────────────────────────────────────────
NOTE = {"C": 0, "D": 2, "E": 4, "F": 5, "G": 7, "A": 9, "B": 11}


def hz(name):
    """'D3' -> Hz, 'Bb2', 'F#3'"""
    n = NOTE[name[0]]
    i = 1
    if name[i] == "#":
        n += 1; i += 1
    elif name[i] == "b":
        n -= 1; i += 1
    octave = int(name[i:])
    return 440.0 * 2 ** ((n + 12 * (octave + 1) - 69) / 12)


# ── buses: stereo float arrays, one per treatment ──────────────────────────
class Bus:
    def __init__(self):
        self.a = np.zeros((N, 2))

    def add(self, sig, t0, gain=1.0, pan=0.0):
        i = int(round(t0 * SR))
        if i >= N or i + len(sig) <= 0:
            return
        s = sig
        if i < 0:
            s = s[-i:]; i = 0
        s = s[: N - i]
        th = (pan + 1) * np.pi / 4  # equal power
        self.a[i : i + len(s), 0] += s * gain * np.cos(th)
        self.a[i : i + len(s), 1] += s * gain * np.sin(th)


music, pads, bells, sfx, ui = Bus(), Bus(), Bus(), Bus(), Bus()


def tt(dur):
    return np.arange(int(dur * SR)) / SR


def env_ad(dur, a, tau):
    t = tt(dur)
    return np.minimum(1, t / max(a, 1e-4)) * np.exp(-t / tau)


def env_ar(dur, a, r):
    t = tt(dur)
    e = np.minimum(1, t / max(a, 1e-4)) * np.minimum(1, (dur - t) / max(r, 1e-4))
    return np.clip(e, 0, 1) ** 1.5


def lp(x, fc, order=2):
    sos = signal.butter(order, min(fc, SR / 2 - 100), "low", fs=SR, output="sos")
    return signal.sosfilt(sos, x)


def hp(x, fc, order=2):
    sos = signal.butter(order, fc, "high", fs=SR, output="sos")
    return signal.sosfilt(sos, x)


def bp(x, lo, hi, order=2):
    sos = signal.butter(order, [lo, min(hi, SR / 2 - 100)], "band", fs=SR, output="sos")
    return signal.sosfilt(sos, x)


def sweep_filter(x, f0, f1, kind="low", block=1024, q=None):
    """Time-varying filter by blocks, state carried across blocks."""
    out = np.zeros_like(x)
    zi = None
    nb = (len(x) + block - 1) // block
    for b in range(nb):
        fc = f0 * (f1 / f0) ** (b / max(1, nb - 1))
        if kind == "band":
            sos = signal.butter(2, [fc * 0.7, fc * 1.4], "band", fs=SR, output="sos")
        else:
            sos = signal.butter(2, min(fc, SR / 2 - 100), kind, fs=SR, output="sos")
        if zi is None or zi.shape[0] != sos.shape[0]:
            zi = np.zeros((sos.shape[0], 2))
        seg = x[b * block : (b + 1) * block]
        y, zi = signal.sosfilt(sos, seg, zi=zi)
        out[b * block : (b + 1) * block] = y
    return out


def noise(dur):
    return rng.standard_normal(int(dur * SR))


# ── instruments ─────────────────────────────────────────────────────────────
def pad_note(freq, dur, bright=1800, detune=(-7, 0, 6), vib=0.0, a=1.2, r=1.6):
    t = tt(dur)
    y = np.zeros_like(t)
    for c in detune:
        f = freq * 2 ** (c / 1200)
        ph = 2 * np.pi * f * t + (vib * np.sin(2 * np.pi * 4.7 * t + c) if vib else 0)
        y += signal.sawtooth(ph) * 0.34
    y = lp(y, bright)
    return y * env_ar(dur, a, r)


def sub(freq, dur, a=0.4, r=0.8, gain=1.0):
    t = tt(dur)
    return (np.sin(2 * np.pi * freq * t) + 0.18 * np.sin(4 * np.pi * freq * t)) * env_ar(dur, a, r) * gain


def bell(freq, dur=3.0, tau=1.1, bright=1.0):
    t = tt(dur)
    parts = [(1.0, 1.0, 1.0), (2.0, 0.42, 0.7), (2.76, 0.3, 0.45), (5.4, 0.12 * bright, 0.25), (8.9, 0.05 * bright, 0.12)]
    y = sum(a * np.sin(2 * np.pi * freq * m * t) * np.exp(-t / (tau * d)) for m, a, d in parts)
    y *= np.minimum(1, t / 0.004)
    return y * 0.5


def pluck(freq, dur=0.9, tau=0.22):
    t = tt(dur)
    y = np.sin(2 * np.pi * freq * t) + 0.35 * np.sin(4 * np.pi * freq * t) * np.exp(-t / (tau * 0.5)) + 0.12 * np.sin(6 * np.pi * freq * t) * np.exp(-t / (tau * 0.3))
    return y * np.exp(-t / tau) * np.minimum(1, t / 0.003) * 0.6


def kick(gain=1.0):
    t = tt(0.55)
    f = 46 + 110 * np.exp(-t / 0.028)
    ph = 2 * np.pi * np.cumsum(f) / SR
    y = np.sin(ph) * np.exp(-t / 0.16)
    y += 0.25 * hp(noise(0.55), 2500) * np.exp(-t / 0.004)
    return y * gain


def hat(open_=False):
    d = 0.22 if open_ else 0.06
    t = tt(d)
    return hp(noise(d), 7000) * np.exp(-t / (0.07 if open_ else 0.012)) * 0.5


def impact(size=1.0):
    d = 3.2
    t = tt(d)
    boom = np.sin(2 * np.pi * (34 + 40 * np.exp(-t / 0.18)) * t) * np.exp(-t / 1.15)
    body = lp(noise(d), 700) * np.exp(-t / 0.5) * 0.9
    crack = hp(noise(d), 1800) * np.exp(-t / 0.045) * 0.7
    return (boom * 1.0 + body * 0.55 + crack * 0.5) * size


def riser(dur, f0=250, f1=9000, gain=1.0):
    n = noise(dur)
    y = sweep_filter(n, f0, f1, "band", block=2048)
    t = tt(dur)
    y *= (t / dur) ** 2.2
    return y * gain * 6


def whoosh(dur=0.9, up=True, f0=400, f1=5000, gain=1.0):
    n = noise(dur)
    y = sweep_filter(n, f0 if up else f1, f1 if up else f0, "band", block=1024)
    t = tt(dur)
    e = np.sin(np.pi * np.clip(t / dur, 0, 1)) ** 2
    return y * e * gain * 6


def click(gain=1.0, pitch=1900):
    t = tt(0.09)
    y = hp(noise(0.09), 2500) * np.exp(-t / 0.004) + 0.5 * np.sin(2 * np.pi * pitch * t) * np.exp(-t / 0.018)
    return y * gain * 0.6


def keyclick(vel=1.0):
    t = tt(0.05)
    y = bp(noise(0.05), 1800, 5200) * np.exp(-t / 0.006) + 0.35 * np.sin(2 * np.pi * (150 + 40 * vel) * t) * np.exp(-t / 0.012)
    return y * vel * 0.5


def blip(f0, f1, dur=0.14, gain=1.0):
    t = tt(dur)
    f = f0 * (f1 / f0) ** (t / dur)
    ph = 2 * np.pi * np.cumsum(f) / SR
    return np.sin(ph) * env_ad(dur, 0.004, dur * 0.35) * gain * 0.6


def tone_stack(freqs, dur, bright=2600, gain=1.0, a=0.8, r=1.2):
    y = sum(pad_note(f, dur, bright=bright, a=a, r=r) for f in freqs)
    return y * gain / max(1, len(freqs)) ** 0.7


# ── musical material ────────────────────────────────────────────────────────
CH = {
    "Dm9": ["D2", "A2", "F3", "A3", "C4", "E4"],
    "Bbmaj7": ["Bb1", "F2", "A3", "D4", "F4", "A4"],
    "Fmaj7": ["F2", "C3", "A3", "C4", "E4", "A4"],
    "Csus": ["C2", "G2", "E3", "G3", "D4", "E4"],
    "Gm9": ["G2", "D3", "Bb3", "D4", "F4", "A4"],
    "Ebmaj7": ["Eb2", "Bb2", "D4", "G4", "Bb4", "D5"],
    "Dmaj": ["D2", "A2", "D3", "F#3", "A3", "E4"],
}
PENTA = ["D5", "F5", "G5", "A5", "C6", "D6", "F6"]


def chord_pad(name, t0, dur, bright=2000, gain=0.5, octave_up_top=False):
    freqs = [hz(n) for n in CH[name]]
    y = np.zeros(int(dur * SR))
    for k, f in enumerate(freqs):
        y += pad_note(f, dur, bright=bright * (1.0 if k < 3 else 1.3), a=min(1.4, dur * 0.3), r=min(1.8, dur * 0.4)) * (1.0 if k else 0.8)
    y *= gain / len(freqs) ** 0.6
    pads.add(y, t0, 1.0, pan=0)
    root = hz(CH[name][0])
    music.add(sub(root, dur, a=0.5, r=min(1.0, dur * 0.3), gain=0.34), t0, 1.0)


def beat_grid(id_, beats):
    s = S[id_]
    d = (s["t1"] - s["t0"]) / beats
    return [s["t0"] + i * d for i in range(beats)], d


def drums(id_, beats, kick_gain=0.8, hats=True, open_hat=False, start_beat=0, end_beat=None):
    grid, d = beat_grid(id_, beats)
    end_beat = end_beat if end_beat is not None else beats
    for i, t in enumerate(grid):
        if i < start_beat or i >= end_beat:
            continue
        music.add(kick(kick_gain), t)
        if hats:
            music.add(hat(open_hat and i % 4 == 2), t + d / 2, 0.45 if i % 2 == 0 else 0.36, pan=0.25)
            if i % 4 == 3:
                music.add(hat(), t + d * 0.75, 0.25, pan=-0.2)


def arp(id_, beats, chord_seq, octave=1, gain=0.22, bars_per_chord=2, start_beat=0, pattern=(0, 2, 4, 2, 5, 4, 2, 3), end_beat=None):
    grid, d = beat_grid(id_, beats)
    end_beat = end_beat if end_beat is not None else beats
    step = d / 2
    for j in range(2 * beats):
        t = grid[0] + j * step
        if j / 2 < start_beat or j / 2 >= end_beat:
            continue
        chord = CH[chord_seq[(j // (4 * bars_per_chord)) % len(chord_seq)]]
        n = chord[1:][pattern[j % len(pattern)] % (len(chord) - 1)]
        f = hz(n) * (2 if octave >= 1 else 1)
        v = 1.0 if j % 4 == 0 else 0.7
        pads.add(pluck(f, 0.7, 0.2), t, gain * v, pan=-0.4 + 0.8 * ((j % 8) / 7))


# ═══════════════════════════ the score ═════════════════════════════════════
t_title, t_cus, t_lim, t_proof, t_ultra, t_mod, t_new, t_cta = [S[k]["t0"] for k in ("title", "custody", "limits", "proof", "ultra", "models", "new", "cta")]
END = cues["duration"]

# ── 0 · hook: a drone, a pulse, three questions, a riser ──
music.add(sub(hz("D1"), 7.0, a=2.4, r=1.0, gain=0.6), 0.0)
pads.add(pad_note(hz("D2"), 7.0, bright=520, a=3.0, r=0.4) * 0.5, 0.0)
pads.add(pad_note(hz("A2"), 7.0, bright=600, a=3.4, r=0.4) * 0.32, 0.4)
for i in range(5):  # a slow heartbeat, the agent waking
    t = 1.0 + i * 0.86
    music.add(np.sin(2 * np.pi * 52 * tt(0.5)) * np.exp(-tt(0.5) / 0.12) * 0.55, t)
for k, (t, n) in enumerate(zip(cues["hook"]["q"], ["D4", "F4", "A4"])):
    bells.add(bell(hz(n), 3.2, 1.0), t, 0.55 + 0.12 * k, pan=-0.2 + 0.2 * k)
    music.add(sub(hz(["D2", "F2", "A2"][k]), 1.0, a=0.02, r=0.8, gain=0.5), t)
r0 = cues["hook"]["riserFrom"]
sfx.add(riser(t_title - r0 + 0.04, 300, 9500, 0.55), r0)

# ── 1 · title: the hit, the chord, the first shimmer ──
sfx.add(impact(1.15), t_title - 0.02, 0.9)
bells.add(bell(hz("D3"), 5.0, 1.6) * 0.8, t_title, 0.6)
chord_pad("Dm9", t_title, 3.2, bright=2200, gain=0.6)
chord_pad("Bbmaj7", t_title + 3.0, 3.4, bright=2600, gain=0.6)
for j, n in enumerate(["D5", "F5", "A5", "C6", "A5", "F5", "D5", "A5"]):
    bells.add(pluck(hz(n), 0.9, 0.3), t_title + 2.2 + j * 0.3, 0.16, pan=-0.5 + 0.14 * j)

# ── cut helper: reversed-cymbal swell into a thump at every cut ──
def cut(t, size=0.55, swell=0.9):
    sfx.add(whoosh(swell, True, 500, 7000, 0.6), t - swell + 0.03)
    sfx.add(impact(size), t, 0.7)


# ── 2 · custody: a beat arrives; the human approves; Auto ──
cut(t_cus)
seq = ["Dm9", "Bbmaj7", "Fmaj7", "Csus"]
bar = (S["custody"]["t1"] - t_cus) / 20 * 8  # 8 beats per chord
for i in range(3):
    chord_pad(seq[i], t_cus + i * bar, bar + 0.8, bright=2400, gain=0.58)
drums("custody", 20, 0.7, hats=True)
arp("custody", 20, seq, gain=0.2)
c = cues["custody"]
for t in (c["propose1"], c["propose2"]):
    sfx.add(blip(500, 1500, 0.16, 1.0), t, 0.35)
    sfx.add(whoosh(0.9, True, 800, 3500, 0.2), t)
for t in (c["arrive1"], c["arrive2"]):
    ui.add(bell(hz("A5"), 0.9, 0.35) * 0.45, t, 0.5)
    ui.add(bell(hz("E5"), 0.9, 0.35) * 0.4, t + 0.13, 0.4)
ui.add(click(1.0), c["approve"] - 0.005)
ui.add(blip(1400, 2100, 0.07, 0.8), c["approve"] + 0.012)
sfx.add(whoosh(1.15, True, 600, 4200, 0.38), c["approve"] + 0.05)
ui.add(click(0.8, 1500), c["autoSeg"])
ui.add(blip(380, 520, 0.12, 0.7), c["autoSeg"] + 0.02)
ui.add(click(1.0), c["autoOn"] - 0.005)
sfx.add(blip(180, 420, 0.5, 0.9), c["autoOn"] + 0.02, 0.5)
ui.add(click(0.55, 1700), c["approve2"])
sfx.add(whoosh(1.15, True, 600, 4200, 0.3), c["approve2"] + 0.05)
for t, notes in ((c["land1"], ["D5", "A5", "E6"]), (c["land2"], ["F5", "C6", "A6"])):
    for k, n in enumerate(notes):
        bells.add(bell(hz(n), 2.4, 0.9), t + 0.07 * k, 0.5, pan=-0.3 + 0.3 * k)

# ── 3 · limits: pressure builds, then the rings stop ──
cut(t_lim)
halt = cues["limits"]["halt"]
bar = (S["limits"]["t1"] - t_lim) / 18 * 8
for i, name in enumerate(["Gm9", "Bbmaj7", "Dm9"]):
    chord_pad(name, t_lim + i * bar, bar + 0.8, bright=2600, gain=0.55)
drums("limits", 18, 0.75, hats=True, open_hat=True)
arp("limits", 18, ["Gm9", "Bbmaj7", "Dm9"], gain=0.2, pattern=(0, 3, 1, 4, 2, 5, 3, 1))
for t, n, cps in [(a["at"], a["n"], a["cps"]) for a in cues["typing"] if a["at"] < t_proof]:
    for k in range(n):
        ui.add(keyclick(0.55 + 0.45 * rng.random()), t + k / cps, 0.55, pan=0.35)
st = cues["limits"]["start"]
sfx.add(blip(120, 520, 0.7, 1.0), st, 0.5)
ui.add(click(0.9), st - 0.005)
# data ticks that speed up and climb until the limit is hit
t, k = st + 0.2, 0
scale = [hz(n) for n in ["D5", "F5", "G5", "A5", "C6", "D6", "F6", "G6", "A6"]]
while t < halt - 0.05:
    p = (t - st) / max(0.1, halt - st)
    ui.add(blip(scale[(k * 2) % len(scale)], scale[(k * 2) % len(scale)] * 1.01, 0.05, 0.5), t, 0.3 + 0.35 * p, pan=-0.4 + 0.8 * rng.random())
    t += 0.19 - 0.12 * p
    k += 1
riser_t = halt - st
sfx.add(riser(riser_t, 400, 6500, 0.28), st)
# the stop: everything ducks, an amber alarm, a hit, a held breath
sfx.add(impact(1.25), halt, 0.9)
for k in range(2):
    t = tt(0.26)
    alarm = (signal.square(2 * np.pi * 311 * t) * 0.5 + np.sin(2 * np.pi * 466 * t)) * env_ad(0.26, 0.004, 0.08)
    sfx.add(lp(alarm, 2400), halt + 0.04 + k * 0.3, 0.28)
chord_pad("Dm9", halt + 0.55, 3.0, bright=1500, gain=0.5)

# ── 4 · proof: glass, one claim at a time ──
cut(t_proof)
bar = (S["proof"]["t1"] - t_proof) / 18 * 8
for i, name in enumerate(["Fmaj7", "Dm9", "Bbmaj7"]):
    chord_pad(name, t_proof + i * bar, bar + 0.8, bright=2800, gain=0.5)
drums("proof", 18, 0.55, hats=False, start_beat=4)
arp("proof", 18, ["Fmaj7", "Dm9", "Bbmaj7"], gain=0.18, pattern=(1, 3, 5, 3, 2, 4, 5, 4), start_beat=2)
for t, n, cps in [(a["at"], a["n"], a["cps"]) for a in cues["typing"] if a["at"] >= t_proof]:
    for k in range(n):
        ui.add(keyclick(0.5 + 0.5 * rng.random()), t + k / cps, 0.5, pan=-0.35)
climb = ["D5", "F5", "A5", "C6", "D6", "F6", "A6", "C7"]
for i, t in enumerate(cues["proof"]["claims"]):
    bells.add(bell(hz(climb[i]), 2.6, 0.7, 1.4), t, 0.42 + 0.03 * i, pan=-0.35 + 0.1 * i)
    ui.add(click(0.35, 2600), t)
d = cues["proof"]["done"]
sfx.add(impact(0.5), d, 0.6)
chord_pad("Fmaj7", d, 4.4, bright=3200, gain=0.55)
for k, n in enumerate(["F5", "A5", "C6", "E6", "A6"]):
    bells.add(bell(hz(n), 3.2, 1.2, 1.2), d + 0.08 * k, 0.45, pan=-0.4 + 0.2 * k)

# ── 5 · ultra: the champagne tier, a rising ladder ──
cut(t_ultra, 0.65)
bar = (S["ultra"]["t1"] - t_ultra) / 14 * 8
chord_pad("Bbmaj7", t_ultra, bar + 0.8, bright=3000, gain=0.6)
chord_pad("Gm9", t_ultra + bar * 0.75, bar + 1.2, bright=3200, gain=0.6)
chord_pad("Ebmaj7", t_ultra + bar * 1.45, 4.0, bright=3600, gain=0.6)
drums("ultra", 14, 0.75, hats=True, open_hat=True)
arp("ultra", 14, ["Bbmaj7", "Gm9", "Ebmaj7"], gain=0.2, bars_per_chord=1)
ladder = ["D3", "F3", "A3", "D4", "Bb4"]
for i, t in enumerate(cues["ultra"]["steps"]):
    stab = tone_stack([hz(ladder[i]), hz(ladder[i]) * 2], 1.4, bright=3200, gain=0.55, a=0.03, r=1.0)
    music.add(stab, t, 0.9)
    sfx.add(blip(hz(ladder[i]), hz(ladder[i]) * 2, 0.3, 0.7), t, 0.35)
sfx.add(whoosh(1.6, True, 300, 3200, 0.3), cues["ultra"]["steps"][3] - 0.1)  # the verifier's sweep
sfx.add(impact(0.9), cues["ultra"]["steps"][4], 0.9)
for k, n in enumerate(["Bb4", "D5", "F5", "A5", "D6"]):
    bells.add(bell(hz(n), 3.4, 1.3), cues["ultra"]["steps"][4] + 0.09 * k, 0.45, pan=-0.4 + 0.2 * k)

# ── 6 · models: thirteen stars ──
cut(t_mod, 0.5)
chord_pad("Fmaj7", t_mod, 4.8, bright=3400, gain=0.5)
m0, m1 = cues["models"]["from"], cues["models"]["to"]
for i in range(13):
    t = m0 + (m1 - m0) * (i / 12) ** 0.9
    bells.add(bell(hz(PENTA[(i * 3) % len(PENTA)]), 2.2, 0.6, 1.5), t, 0.34, pan=rng.uniform(-0.8, 0.8))
drums("models", 8, 0.5, hats=False, start_beat=0, end_beat=4)

# ── 7 · what's new: warm and light ──
cut(t_new, 0.35)
chord_pad("Bbmaj7", t_new, 5.4, bright=2600, gain=0.5)
sfx.add(whoosh(1.0, False, 600, 4200, 0.3), cues["new"]["shot"])
for i, t in enumerate(cues["new"]["items"]):
    ui.add(blip(700 + 120 * i, 1100 + 150 * i, 0.09, 0.8), t, 0.5)
    bells.add(bell(hz(PENTA[i]), 1.6, 0.5, 1.0), t, 0.3)
arp("new", 8, ["Bbmaj7"], gain=0.17, start_beat=0)

# ── 8 · call to action: it resolves into the light ──
cut(t_cta, 0.9, swell=1.0)
chord_pad("Dmaj", t_cta, 8.8, bright=3600, gain=0.72)
bells.add(bell(hz("D4"), 6.0, 2.0) * 0.8, t_cta + 0.05, 0.6)
for k, n in enumerate(["F#5", "A5", "D6", "F#6"]):
    bells.add(bell(hz(n), 4.0, 1.6), t_cta + 0.2 + 0.12 * k, 0.4, pan=-0.4 + 0.27 * k)
for j, n in enumerate(["D5", "F#5", "A5", "E5", "F#5", "A5", "D6", "A5"] * 3):
    bells.add(pluck(hz(n), 0.8, 0.28), t_cta + 1.2 + j * 0.3, 0.14, pan=-0.5 + 0.14 * (j % 8))
for t in cues["cta"]["ticks"]:
    ui.add(blip(900, 1500, 0.08, 0.8), t, 0.45)
grid, dd = beat_grid("cta", 14)
for i, t in enumerate(grid[:8]):
    music.add(kick(0.55), t)
sfx.add(impact(0.5), END - 1.2, 0.4)  # a last low breath under the fade


# ── ducking: the music bus gets out of the way of the big moments ──
def duck_curve(events):
    g = np.ones(N)
    for t, depth, hold, rel in events:
        i0 = int(t * SR)
        a = int(0.03 * SR)
        h = int(hold * SR)
        r = int(rel * SR)
        env = np.concatenate([np.linspace(1, 1 - depth, a), np.full(h, 1 - depth), np.linspace(1 - depth, 1, r)])
        j = min(N, i0 + len(env))
        g[i0:j] = np.minimum(g[i0:j], env[: j - i0])
    return g


duck = duck_curve([(halt - 0.02, 1.0, 0.55, 1.2), (t_title - 0.03, 0.5, 0.2, 1.2)])
music.a *= duck[:, None]
pads.a *= duck_curve([(halt - 0.02, 1.0, 0.5, 1.0)])[:, None]


# ── reverb: a synthetic stereo hall, per bus ──
def hall(tau=1.5, pre=0.018, bright=5500, length=3.6):
    n = int(length * SR)
    t = np.arange(n) / SR
    ir = np.zeros((n, 2))
    for ch in range(2):
        x = rng.standard_normal(n) * np.exp(-t / tau)
        x = lp(x, bright)
        ir[:, ch] = x
    ir[: int(pre * SR)] = 0
    ir /= np.sqrt(np.sum(ir ** 2, axis=0, keepdims=True))
    return ir


def reverb(bus, wet, ir):
    out = np.zeros_like(bus.a)
    for ch in range(2):
        out[:, ch] = signal.fftconvolve(bus.a[:, ch], ir[:, ch], mode="full")[:N]
    return bus.a * (1 - wet * 0.35) + out * wet


HALL = hall(1.5)
SHORT = hall(0.45, 0.006, 7500, 1.2)
mix = (
    reverb(music, 0.22, HALL) * 0.95
    + reverb(pads, 0.38, HALL) * 0.92
    + reverb(bells, 0.55, HALL) * 1.0
    + reverb(sfx, 0.3, HALL) * 0.95
    + reverb(ui, 0.12, SHORT) * 0.85
)

# gentle master: fade in/out, soft clip, trim the tail
t = np.arange(N) / SR
fade_in = np.clip(t / 1.3, 0, 1) ** 1.5
fade_out = np.clip((END - t) / 1.4, 0, 1) ** 1.5
mix *= (fade_in * fade_out)[:, None]
mix = hp(mix.T, 28).T
peak = np.max(np.abs(mix))
mix = np.tanh(mix / peak * 1.35) / np.tanh(1.35) * 0.89
mix = mix[: int(END * SR)]
wavfile.write(OUT, SR, (mix * 32767).astype(np.int16))
print(f"wrote {OUT}: {len(mix) / SR:.1f} s, peak {np.max(np.abs(mix)):.3f}")
