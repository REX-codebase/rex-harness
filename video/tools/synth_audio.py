#!/usr/bin/env python3
"""Synthesizes the film's score and sound design, and mixes in the narration.

    python3 tools/synth_audio.py cues.json vo_dir out.wav

No samples, no stock music. Everything is generated from the cue sheet (tools/print-cues.mjs),
so the sound is locked to the picture: the strike lands on the frame the ring locks, the silence
lands on the frame the run hits its limit. The narration (tools/narrate.py) is mixed on top and
the music ducks under it.  Needs numpy and scipy.
"""
import json
import sys

import numpy as np
from scipy import signal
from scipy.io import wavfile

SR = 48000
rng = np.random.default_rng(11)

cues = json.load(open(sys.argv[1]))
VO_DIR = sys.argv[2]
OUT = sys.argv[3]
vo_lines = json.load(open(f"{VO_DIR}/vo.json"))
END = cues["duration"]
N = int((END + 2.0) * SR)
S = cues["shots"]
T0 = {k: v["t0"] for k, v in S.items()}

NOTE = {"C": 0, "D": 2, "E": 4, "F": 5, "G": 7, "A": 9, "B": 11}


def hz(name):
    n = NOTE[name[0]]
    i = 1
    if name[i] == "#":
        n += 1; i += 1
    elif name[i] == "b":
        n -= 1; i += 1
    return 440.0 * 2 ** ((n + 12 * (int(name[i:]) + 1) - 69) / 12)


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
        th = (pan + 1) * np.pi / 4
        self.a[i : i + len(s), 0] += s * gain * np.cos(th)
        self.a[i : i + len(s), 1] += s * gain * np.sin(th)


music, pads, bells, hits, ui, voice = Bus(), Bus(), Bus(), Bus(), Bus(), Bus()


def tt(dur):
    return np.arange(int(dur * SR)) / SR


def env_ar(dur, a, r):
    t = tt(dur)
    return np.clip(np.minimum(1, t / max(a, 1e-4)) * np.minimum(1, (dur - t) / max(r, 1e-4)), 0, 1) ** 1.5


def env_ad(dur, a, tau):
    t = tt(dur)
    return np.minimum(1, t / max(a, 1e-4)) * np.exp(-t / tau)


def lp(x, fc, order=2):
    return signal.sosfilt(signal.butter(order, min(fc, SR / 2 - 100), "low", fs=SR, output="sos"), x)


def hp(x, fc, order=2):
    return signal.sosfilt(signal.butter(order, fc, "high", fs=SR, output="sos"), x)


def bp(x, lo, hi, order=2):
    return signal.sosfilt(signal.butter(order, [lo, min(hi, SR / 2 - 100)], "band", fs=SR, output="sos"), x)


def sweep(x, f0, f1, kind="band", block=1024):
    out = np.zeros_like(x)
    zi = None
    nb = (len(x) + block - 1) // block
    for b in range(nb):
        fc = f0 * (f1 / f0) ** (b / max(1, nb - 1))
        sos = signal.butter(2, [fc * 0.7, min(fc * 1.4, SR / 2 - 100)], "band", fs=SR, output="sos") if kind == "band" else signal.butter(2, min(fc, SR / 2 - 100), kind, fs=SR, output="sos")
        if zi is None or zi.shape[0] != sos.shape[0]:
            zi = np.zeros((sos.shape[0], 2))
        y, zi = signal.sosfilt(sos, x[b * block : (b + 1) * block], zi=zi)
        out[b * block : (b + 1) * block] = y
    return out


def noise(dur):
    return rng.standard_normal(int(dur * SR))


# ── instruments ─────────────────────────────────────────────────────────────
def pad_note(freq, dur, bright=1800, detune=(-8, 0, 7), a=1.2, r=1.6):
    t = tt(dur)
    y = sum(signal.sawtooth(2 * np.pi * freq * 2 ** (c / 1200) * t) * 0.34 for c in detune)
    return lp(y, bright) * env_ar(dur, a, r)


def sub(freq, dur, a=0.4, r=0.8, gain=1.0):
    t = tt(dur)
    return (np.sin(2 * np.pi * freq * t) + 0.16 * np.sin(4 * np.pi * freq * t)) * env_ar(dur, a, r) * gain


def bell(freq, dur=3.0, tau=1.1, bright=1.0):
    t = tt(dur)
    parts = [(1.0, 1.0, 1.0), (2.0, 0.42, 0.7), (2.76, 0.3, 0.45), (5.4, 0.12 * bright, 0.25), (8.9, 0.05 * bright, 0.12)]
    return sum(a * np.sin(2 * np.pi * freq * m * t) * np.exp(-t / (tau * d)) for m, a, d in parts) * np.minimum(1, t / 0.004) * 0.5


def pluck(freq, dur=0.9, tau=0.22):
    t = tt(dur)
    y = np.sin(2 * np.pi * freq * t) + 0.35 * np.sin(4 * np.pi * freq * t) * np.exp(-t / (tau * 0.5))
    return y * np.exp(-t / tau) * np.minimum(1, t / 0.003) * 0.6


def kick(gain=1.0):
    t = tt(0.55)
    ph = 2 * np.pi * np.cumsum(46 + 110 * np.exp(-t / 0.028)) / SR
    return (np.sin(ph) * np.exp(-t / 0.16) + 0.25 * hp(noise(0.55), 2500) * np.exp(-t / 0.004)) * gain


def hat():
    return hp(noise(0.06), 7000) * np.exp(-tt(0.06) / 0.012) * 0.5


def impact(size=1.0):
    d = 3.4
    t = tt(d)
    boom = np.sin(2 * np.pi * (33 + 42 * np.exp(-t / 0.18)) * t) * np.exp(-t / 1.2)
    body = lp(noise(d), 700) * np.exp(-t / 0.5) * 0.9
    crack = hp(noise(d), 1800) * np.exp(-t / 0.045) * 0.7
    return (boom + 0.55 * body + 0.5 * crack) * size


def metal(freq, dur=2.6, size=1.0):
    """A struck metal ring: inharmonic partials, a noise transient, a low thud."""
    t = tt(dur)
    parts = [(1.0, 1.0, 1.0), (2.32, 0.7, 0.8), (3.91, 0.55, 0.6), (5.60, 0.4, 0.45), (7.93, 0.28, 0.3), (10.4, 0.18, 0.2)]
    y = sum(a * np.sin(2 * np.pi * freq * m * t + 0.3 * m) * np.exp(-t / (1.3 * d)) for m, a, d in parts) * 0.32
    y += bp(noise(dur), 1500, 7000) * np.exp(-t / 0.03) * 0.9
    y += np.sin(2 * np.pi * (60 + 80 * np.exp(-t / 0.06)) * t) * np.exp(-t / 0.35) * 0.9
    return y * np.minimum(1, t / 0.0015) * size


def riser(dur, f0=250, f1=9000, gain=1.0):
    n = sweep(noise(dur), f0, f1, "band", 2048)
    return n * (tt(dur) / dur) ** 2.2 * gain * 6


def whoosh(dur=0.9, up=True, f0=400, f1=5000, gain=1.0):
    n = sweep(noise(dur), f0 if up else f1, f1 if up else f0, "band", 1024)
    return n * np.sin(np.pi * np.clip(tt(dur) / dur, 0, 1)) ** 2 * gain * 6


def glide(f0, f1, dur, gain=1.0):
    t = tt(dur)
    f = f0 * (f1 / f0) ** (t / dur)
    return np.sin(2 * np.pi * np.cumsum(f) / SR) * env_ar(dur, dur * 0.3, dur * 0.3) * gain


def click(gain=1.0, pitch=1900):
    t = tt(0.09)
    return (hp(noise(0.09), 2500) * np.exp(-t / 0.004) + 0.5 * np.sin(2 * np.pi * pitch * t) * np.exp(-t / 0.018)) * gain * 0.6


def keyclick(vel=1.0):
    t = tt(0.05)
    return (bp(noise(0.05), 1800, 5200) * np.exp(-t / 0.006) + 0.35 * np.sin(2 * np.pi * (150 + 40 * vel) * t) * np.exp(-t / 0.012)) * vel * 0.5


def blip(f0, f1, dur=0.14, gain=1.0):
    t = tt(dur)
    f = f0 * (f1 / f0) ** (t / dur)
    return np.sin(2 * np.pi * np.cumsum(f) / SR) * env_ad(dur, 0.004, dur * 0.35) * gain * 0.6


def tick_wood(gain=1.0, f=1100):
    t = tt(0.12)
    return (np.sin(2 * np.pi * f * t) * np.exp(-t / 0.012) + 0.4 * np.sin(2 * np.pi * f * 2.4 * t) * np.exp(-t / 0.008) + 0.3 * bp(noise(0.12), 2000, 6000) * np.exp(-t / 0.004)) * gain * 0.7


def thump(freq=52, gain=1.0):
    t = tt(0.6)
    return np.sin(2 * np.pi * (freq + 30 * np.exp(-t / 0.05)) * t) * np.exp(-t / 0.13) * gain


CH = {
    "Dm9": ["D2", "A2", "F3", "A3", "C4", "E4"], "Bbmaj7": ["Bb1", "F2", "A3", "D4", "F4", "A4"],
    "Fmaj7": ["F2", "C3", "A3", "C4", "E4", "A4"], "Gm9": ["G2", "D3", "Bb3", "D4", "F4", "A4"],
    "Ebmaj7": ["Eb2", "Bb2", "D4", "G4", "Bb4", "D5"], "Dmaj": ["D2", "A2", "D3", "F#3", "A3", "E4"],
    "Dsus": ["D2", "A2", "E3", "A3", "D4", "E4"],
}


def chord_pad(name, t0, dur, bright=2000, gain=0.5, bus=None, a=None, r=None):
    freqs = [hz(n) for n in CH[name]]
    y = np.zeros(int(dur * SR))
    for k, f in enumerate(freqs):
        y += pad_note(f, dur, bright=bright * (1.0 if k < 3 else 1.3), a=a if a else min(1.4, dur * 0.3), r=r if r else min(1.8, dur * 0.4)) * (1.0 if k else 0.8)
    (bus or pads).add(y * gain / len(freqs) ** 0.6, t0, 1.0)
    music.add(sub(hz(CH[name][0]), dur, a=0.5, r=min(1.0, dur * 0.3), gain=0.32), t0, 1.0)


def arp(t0, t1, chord, step, gain=0.18, pattern=(0, 2, 4, 2, 5, 4, 2, 3), oct_up=True):
    notes = CH[chord][1:]
    t, j = t0, 0
    while t < t1 - 0.2:
        f = hz(notes[pattern[j % len(pattern)] % len(notes)]) * (2 if oct_up else 1)
        pads.add(pluck(f, 0.7, 0.2), t, gain * (1.0 if j % 4 == 0 else 0.7), pan=-0.4 + 0.8 * ((j % 8) / 7))
        t += step; j += 1


# ═══════════════════════════ the score ═════════════════════════════════════
H = cues["harness"]
L = cues["limits"]
G = cues["gate"]
P = cues["proof"]
U = cues["ultra"]
M = cues["models"]
F = cues["finale"]

# ── 0 · open: black, a heartbeat, something enormous breathing ─────────────
music.add(sub(hz("D1"), 8.0, a=4.5, r=1.5, gain=0.7), 0.0)
pads.add(pad_note(hz("D2"), 8.0, bright=420, a=4.5, r=1.2) * 0.55, 0.4)
pads.add(pad_note(hz("A2"), 8.0, bright=520, a=5.0, r=1.2) * 0.34, 1.0)
pads.add(pad_note(hz("E3"), 7.0, bright=700, a=5.0, r=1.2) * 0.2, 2.0)
beat = cues["open"]["beat"]
t = 0.55
while t < T0["reveal"] + 0.6:
    amp = min(1.0, 0.35 + t / 6.0)
    music.add(thump(54, 0.9 * amp), t)
    music.add(thump(48, 0.55 * amp), t + 0.23)
    t += beat
for k, tm in enumerate([2.6, 3.9, 5.1]):
    bells.add(bell(hz(["A6", "E6", "D7"][k]), 3.0, 0.9) * 0.4, tm, 0.12, pan=-0.5 + 0.5 * k)

# ── 1 · reveal: a riser, and three flares that grow ────────────────────────
r0 = T0["reveal"]
hits.add(riser(6.3, 220, 8500, 0.5), r0 + 0.2)
pads.add(glide(110, 880, 6.2, 0.16), r0 + 0.2)
chord_pad("Dm9", r0 + 2.0, 5.6, bright=1600, gain=0.55, a=3.0, r=1.0)
for i, f in enumerate(cues["reveal"]["flares"]):
    size = [0.32, 0.5, 1.0][i]
    hits.add(impact(size), f["at"] - 0.02, 0.85)
    bells.add(bell(hz(["D4", "F4", "A4"][i]), 3.4, 1.2) * 0.8, f["at"], 0.32 + 0.1 * i)
last = cues["reveal"]["flares"][-1]["at"]
music.add(sum(pad_note(hz(n), 2.4, bright=1100, a=0.05, r=1.4) for n in ["D2", "A2", "D3", "F3"]) * 0.25, last - 0.02, 1.0)

# ── 2 · tension: a clock, a pulse, a held breath ───────────────────────────
t2 = T0["tension"]
tn = cues["tension"]["flares"]
t = t2 + 0.3
k = 0
while t < T0["harness"] - 0.5:
    hits.add(tick_wood(0.28 + 0.2 * (t - t2) / 6, 900 + 40 * (k % 4)), t, 1.0, pan=0.3 * (-1) ** k)
    music.add(pluck(hz("D2"), 0.5, 0.18) * 0.7, t + 0.25, 0.55)
    t += 0.5 - 0.1 * (t - t2) / 6
    k += 1
for f in tn:
    hits.add(hp(noise(0.6), 900) * env_ad(0.6, 0.002, 0.12) * 0.7, f["at"], 0.18 * f["a"], pan=rng.uniform(-0.6, 0.6))
    bells.add((bell(hz("D5"), 2.4, 0.5) + bell(hz("Eb5"), 2.4, 0.5)) * 0.35, f["at"], 0.18 * f["a"])
hits.add(impact(0.8), tn[-1]["at"] - 0.02, 0.8)
hits.add(riser(1.0, 300, 7500, 0.55), T0["harness"] - 1.0)
chord_pad("Dsus", t2, 6.4, bright=1500, gain=0.5, a=2.0, r=1.0)

# ── 3 · harness: four rings, four strikes ──────────────────────────────────
h0 = T0["harness"]
hits.add(impact(0.25), h0 - 0.0, 0.6)
music.add(sub(hz("D1"), 3.0, a=0.8, r=1.2, gain=0.55), h0 + 0.2)
notes_lock = ["D2", "F2", "A2", "D3"]
for i, tl in enumerate(H["locks"]):
    hits.add(whoosh(H["flight"], True, 300 + 150 * i, 6000, 0.45), tl - H["flight"] + 0.03, 1.0, pan=-0.3 + 0.2 * i)
    size = 1.0 if i < 3 else 1.5
    hits.add(metal(hz(notes_lock[i]) * 2, 2.8, size), tl, 0.85)
    hits.add(impact(0.5 + 0.12 * i), tl, 0.7)
    bells.add(bell(hz(["D5", "F5", "A5", "D6"][i]), 3.2, 1.1), tl + 0.02, 0.28)
    music.add(sub(hz(notes_lock[i]), 2.2, a=0.01, r=1.4, gain=0.5), tl)
last_lock = H["locks"][-1]
chord_pad("Dm9", last_lock, 5.0, bright=2600, gain=0.7, a=0.25, r=1.8)
hits.add(riser(0.8, 400, 7000, 0.3), last_lock - 0.8)
arp(last_lock + 1.0, T0["limits"] + 0.5, "Dm9", 0.3, gain=0.17)

# ── 4 · limits: the run, and where it stops ────────────────────────────────
l0 = T0["limits"]
halt = L["halt"]
chord_pad("Gm9", l0, halt - l0 + 0.2, bright=2200, gain=0.5, a=0.8, r=0.1)
hits.add(blip(120, 520, 0.7, 1.0), L["start"], 0.5)
ui.add(click(0.9), L["start"] - 0.005)
t, k = L["start"] + 0.2, 0
scale = [hz(n) for n in ["D5", "F5", "G5", "A5", "C6", "D6", "F6", "G6", "A6"]]
while t < halt - 0.05:
    p = (t - L["start"]) / max(0.1, halt - L["start"])
    ui.add(blip(scale[(k * 2) % len(scale)], scale[(k * 2) % len(scale)] * 1.01, 0.05, 0.5), t, 0.28 + 0.35 * p, pan=-0.4 + 0.8 * rng.random())
    t += 0.2 - 0.13 * p
    k += 1
hits.add(riser(halt - L["start"], 400, 6500, 0.28), L["start"])
# kick pattern builds with the run
tk = L["start"]
while tk < halt - 0.1:
    music.add(kick(0.55 + 0.35 * (tk - L["start"]) / (halt - L["start"])), tk)
    tk += 0.6
# the stop: everything ducks, an amber alarm, a hit, a held breath
hits.add(impact(1.3), halt, 0.95)
for k in range(2):
    t = tt(0.26)
    alarm = (signal.square(2 * np.pi * 311 * t) * 0.5 + np.sin(2 * np.pi * 466 * t)) * env_ad(0.26, 0.004, 0.08)
    hits.add(lp(alarm, 2400), halt + 0.05 + k * 0.3, 0.28)
chord_pad("Dm9", halt + 0.7, T0["gate"] - halt + 0.5, bright=1300, gain=0.45, a=1.5, r=1.2)

# ── 5 · gate: suspense, then a decision ────────────────────────────────────
g0 = T0["gate"]
hits.add(impact(0.3), g0, 0.5)
hits.add(blip(500, 1500, 0.16, 1.0), G["propose"], 0.35)
hits.add(whoosh(G["arrive"] - G["propose"], True, 600, 3500, 0.18), G["propose"])
k = 0
t = G["arrive"]
while t < G["approve"] - 0.1:
    hits.add(tick_wood(0.55, 760 if k % 2 == 0 else 640), t, 1.0, pan=0.25)  # tick, tock: it is waiting for you
    t += 0.5; k += 1
bells.add(bell(hz("A5"), 0.9, 0.35) * 0.45, G["arrive"], 0.5)
bells.add(bell(hz("E5"), 0.9, 0.35) * 0.4, G["arrive"] + 0.13, 0.4)
ui.add(click(1.0), G["approve"] - 0.005)
ui.add(blip(1400, 2100, 0.07, 0.8), G["approve"] + 0.012)
hits.add(whoosh(1.15, True, 600, 4200, 0.38), G["approve"] + 0.05)
for k, n in enumerate(["D5", "A5", "E6"]):
    bells.add(bell(hz(n), 2.4, 0.9), G["land"] + 0.07 * k, 0.5, pan=-0.3 + 0.3 * k)
chord_pad("Bbmaj7", G["approve"], T0["proof"] - G["approve"] + 0.6, bright=2000, gain=0.5, a=0.4, r=0.8)
chord_pad("Fmaj7", g0 + 0.3, G["approve"] - g0 + 0.2, bright=1100, gain=0.36, a=1.2, r=0.5)

# ── 6 · proof: glass, one claim at a time ──────────────────────────────────
p0 = T0["proof"]
hits.add(impact(0.3), p0, 0.5)
chord_pad("Fmaj7", p0 + 0.4, P["done"] - p0, bright=2800, gain=0.46, a=1.0, r=0.4)
for tp, n, cps in [(x["at"], x["n"], x["cps"]) for x in cues["typing"]]:
    for k in range(n):
        ui.add(keyclick(0.5 + 0.5 * rng.random()), tp + k / cps, 0.5, pan=-0.35)
pads.add(glide(440, 1760, P["scanTo"] - P["scanFrom"], 0.1), P["scanFrom"])
climb = ["D5", "F5", "A5", "C6", "D6", "F6", "A6", "C7"]
for i, tc in enumerate(P["claims"]):
    bells.add(bell(hz(climb[i]), 2.6, 0.7, 1.4), tc, 0.4 + 0.03 * i, pan=-0.35 + 0.1 * i)
    ui.add(click(0.3, 2600), tc)
d = P["done"]
hits.add(impact(0.6), d, 0.7)
chord_pad("Fmaj7", d, 4.0, bright=3300, gain=0.55, a=0.3, r=1.5)
for k, n in enumerate(["F5", "A5", "C6", "E6", "A6"]):
    bells.add(bell(hz(n), 3.2, 1.2, 1.2), d + 0.08 * k, 0.42, pan=-0.4 + 0.2 * k)

# ── 7 · ultra: the champagne tier, a rising ladder ─────────────────────────
u0 = T0["ultra"]
hits.add(whoosh(0.9, True, 500, 6000, 0.4), u0 - 0.85)
hits.add(impact(0.65), u0, 0.7)
chord_pad("Bbmaj7", u0, 4.0, bright=3000, gain=0.6, a=0.6)
chord_pad("Gm9", u0 + 3.0, 3.6, bright=3200, gain=0.6, a=0.8)
chord_pad("Ebmaj7", u0 + 5.8, 3.4, bright=3600, gain=0.62, a=0.6)
tk = u0
while tk < U["steps"][4]:
    music.add(kick(0.6), tk); tk += 0.6
ladder = ["D3", "F3", "A3", "D4", "Bb4"]
for i, tu in enumerate(U["steps"]):
    music.add(sum(pad_note(f, 1.4, bright=3200, a=0.03, r=1.0) for f in (hz(ladder[i]), hz(ladder[i]) * 2)) * 0.28, tu, 0.9)
    hits.add(blip(hz(ladder[i]), hz(ladder[i]) * 2, 0.3, 0.7), tu, 0.35)
hits.add(whoosh(1.6, True, 300, 3200, 0.3), U["steps"][3] - 0.1)
hits.add(impact(1.0), U["steps"][4], 0.95)
for k, n in enumerate(["Bb4", "D5", "F5", "A5", "D6"]):
    bells.add(bell(hz(n), 3.4, 1.3), U["steps"][4] + 0.09 * k, 0.45, pan=-0.4 + 0.2 * k)

# ── 8 · models: thirteen stars ─────────────────────────────────────────────
m0 = T0["models"]
hits.add(impact(0.4), m0, 0.5)
chord_pad("Fmaj7", m0, 5.0, bright=3400, gain=0.48, a=0.8)
PENTA = ["D5", "F5", "G5", "A5", "C6", "D6", "F6"]
for i in range(13):
    tm = M["from"] + (M["to"] - M["from"]) * (i / 12) ** 0.9
    bells.add(bell(hz(PENTA[(i * 3) % len(PENTA)]), 2.2, 0.6, 1.5), tm, 0.34, pan=rng.uniform(-0.8, 0.8))

# ── 9 · finale: release ────────────────────────────────────────────────────
f0 = T0["finale"]
hits.add(riser(1.9, 250, 9000, 0.55), f0 - 1.9)
hits.add(impact(1.35), f0 + 0.2, 1.0)
chord_pad("Dmaj", f0 + 0.2, 9.8, bright=3600, gain=0.75, a=0.5, r=2.4)
bells.add(bell(hz("D4"), 6.0, 2.0) * 0.8, f0 + 0.3, 0.6)
for k, n in enumerate(["F#5", "A5", "D6", "F#6"]):
    bells.add(bell(hz(n), 4.0, 1.6), F["title"] + 0.12 * k, 0.4, pan=-0.4 + 0.27 * k)
arp(F["tag"] - 0.3, END - 1.5, "Dmaj", 0.3, gain=0.14, pattern=(0, 2, 4, 3, 2, 4, 5, 3))
tk = f0 + 0.2
while tk < F["tag"]:
    music.add(kick(0.55), tk); tk += 0.6
hits.add(bell(hz("D6"), 6.0, 2.4) * 0.5, F["tag"] + 0.6, 0.5)
hits.add(impact(0.5), END - 1.5, 0.4)

# ═══════════════════════════ voice ═════════════════════════════════════════
for v in vo_lines:
    sr, x = wavfile.read(f"{VO_DIR}/{v['id']}.wav")
    voice.add(x.astype(np.float32) / 32767, v["at"], 1.0)


# ═══════════════════════════ mix ═══════════════════════════════════════════
def duck_curve(events):
    g = np.ones(N)
    for t, depth, hold, rel in events:
        i0 = int(t * SR); a = int(0.05 * SR); h = int(hold * SR); r = int(rel * SR)
        env = np.concatenate([np.linspace(1, 1 - depth, a), np.full(h, 1 - depth), np.linspace(1 - depth, 1, r)])
        j = min(N, i0 + len(env))
        g[i0:j] = np.minimum(g[i0:j], env[: j - i0])
    return g


vo_duck = duck_curve([(v["at"] - 0.05, 0.42, v["dur"], 0.45) for v in vo_lines])           # music gets out of the narrator's way
stop_duck = duck_curve([(halt - 0.02, 1.0, 0.65, 1.3), (T0["harness"] - 0.2, 0.9, 0.18, 0.5)])  # and goes silent when the run stops
music.a *= (vo_duck * stop_duck)[:, None]
pads.a *= (vo_duck * stop_duck)[:, None]
bells.a *= (0.6 + 0.4 * vo_duck)[:, None]


def hall(tau=1.5, pre=0.018, bright=5500, length=3.8):
    n = int(length * SR)
    t = np.arange(n) / SR
    ir = np.stack([lp(rng.standard_normal(n) * np.exp(-t / tau), bright) for _ in range(2)], axis=1)
    ir[: int(pre * SR)] = 0
    return ir / np.sqrt(np.sum(ir ** 2, axis=0, keepdims=True))


def reverb(bus, wet, ir):
    out = np.zeros_like(bus.a)
    for ch in range(2):
        out[:, ch] = signal.fftconvolve(bus.a[:, ch], ir[:, ch], mode="full")[:N]
    return bus.a * (1 - wet * 0.35) + out * wet


HALL, SHORT, ROOM = hall(1.6), hall(0.45, 0.006, 7500, 1.2), hall(0.9, 0.012, 6500, 2.0)
mix = (
    reverb(music, 0.22, HALL) * 0.95
    + reverb(pads, 0.4, HALL) * 0.9
    + reverb(bells, 0.55, HALL) * 1.0
    + reverb(hits, 0.32, HALL) * 1.0
    + reverb(ui, 0.12, SHORT) * 0.8
    + reverb(voice, 0.16, ROOM) * 1.55
)
t = np.arange(N) / SR
mix *= (np.clip(t / 1.2, 0, 1) ** 1.5 * np.clip((END - t) / 1.6, 0, 1) ** 1.5)[:, None]
mix = hp(mix.T, 28).T
peak = np.max(np.abs(mix))
mix = np.tanh(mix / peak * 1.3) / np.tanh(1.3) * 0.89
mix = mix[: int(END * SR)]
wavfile.write(OUT, SR, (mix * 32767).astype(np.int16))
print(f"wrote {OUT}: {len(mix) / SR:.1f} s, peak {np.max(np.abs(mix)):.3f}")
