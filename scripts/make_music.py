"""Synthesizes the original soundtrack and sound effects into audio/ (needs numpy).

    python scripts/make_music.py [--seed 1128]

The timing mirrors src/lib.rs: 160 BPM half-time, one bar = 1.5 s.
  0 .. 3 s         intro, two bars: pad + arpeggio, filter opening, hats join
  3 .. 27 s        countdown #5 .. #2, four bars per launch, drums
  26.625 .. 27 s   one beat of silence before #1 (riser underneath)
  27 .. 33 s       #1: full groove plus a bell line
  33 .. 37.5 s     leaderboard: drums thin out, bells, fade

A video with fewer than five launches skips 6 s blocks of the countdown (see `audio()` in
src/lib.rs), so every block starts on a bar line and the #1 section always lands on #1.
"""
import argparse
import os
import wave

import numpy as np

SR = 48000
BPM = 160.0
BEAT = 60.0 / BPM
BAR = 4 * BEAT
INTRO = 3.0
PER_PRODUCT = 6.0
TOP = 5
FIRST = INTRO + (TOP - 1) * PER_PRODUCT  # 27 s, #1 starts
OUTRO = INTRO + TOP * PER_PRODUCT  # 33 s
TOTAL = OUTRO + 3 * BAR
OUT = os.path.join(os.path.dirname(__file__), "..", "audio")
rng = np.random.default_rng(1)


def t_of(n):
    return np.arange(n) / SR


def midi(n):
    return 440.0 * 2 ** ((n - 69) / 12)


def saw(f, t, detune=0.0):
    ph = f * (1 + detune) * t
    return 2 * (ph - np.floor(ph + 0.5))


def one_pole(x, cutoff):
    """One-pole low-pass; cutoff in Hz, scalar or one value per sample."""
    cutoff = np.broadcast_to(np.asarray(cutoff, dtype=float), x.shape)
    a = 1.0 - np.exp(-2 * np.pi * cutoff / SR)
    y = np.empty_like(x)
    acc = 0.0
    for i in range(len(x)):
        acc += a[i] * (x[i] - acc)
        y[i] = acc
    return y


def highpass(x, cutoff):
    return x - one_pole(x, cutoff)


def env(n, a=0.005, d=0.1, s=0.6, r=0.2):
    e = np.full(n, s)
    na, nd, nr = max(int(a * SR), 1), int(d * SR), int(r * SR)
    e[:na] = np.linspace(0, 1, na)
    e[na:na + nd] = np.linspace(1, s, len(e[na:na + nd]))
    if nr:
        e[-nr:] *= np.linspace(1, 0, nr)
    return e


def write(name, x):
    x = np.asarray(x, dtype=float)
    if x.ndim == 1:
        x = np.stack([x, x], 1)
    x = x / (np.max(np.abs(x)) + 1e-9) * 0.89  # about -1 dBFS
    with wave.open(os.path.join(OUT, name), "wb") as w:
        w.setnchannels(2)
        w.setsampwidth(2)
        w.setframerate(SR)
        w.writeframes((x * 32767).astype("<i2").tobytes())
    print("wrote", name, f"{len(x) / SR:.2f}s")


# ------------------------------------------------------------------ instruments
def kick(punch=1.0):
    n = int(0.5 * SR)
    t = t_of(n)
    f = 46 + 120 * np.exp(-t * 30)
    body = np.sin(2 * np.pi * np.cumsum(f) / SR) * np.exp(-t * 6.5)
    click = rng.standard_normal(n) * np.exp(-t * 320) * 0.2
    return (body + click) * punch


def snare():
    n = int(0.32 * SR)
    t = t_of(n)
    body = np.sin(2 * np.pi * 185 * t) * np.exp(-t * 28) * 0.55
    noise = highpass(rng.standard_normal(n), 1400) * np.exp(-t * 13) * 0.55
    return body + noise


def clap():
    n = int(0.3 * SR)
    t = t_of(n)
    e = np.exp(-t * 20)
    for k in (0.0, 0.010, 0.021):
        i = int(k * SR)
        e[i:i + 70] += 0.8
    return highpass(rng.standard_normal(n), 1000) * e * 0.4


def hat(open_=False):
    n = int((0.25 if open_ else 0.05) * SR)
    t = t_of(n)
    return highpass(rng.standard_normal(n), 7500) * np.exp(-t * (12 if open_ else 75)) * 0.22


def bass(note, dur):
    n = int(dur * SR)
    t = t_of(n)
    f = midi(note)
    x = np.sin(2 * np.pi * f * t) + 0.3 * saw(f, t)
    x = one_pole(x, 260 + 600 * np.exp(-t * 8))
    return x * env(n, 0.004, 0.2, 0.75, 0.08) * 0.6


def pad(notes, dur, bright):
    n = int(dur * SR)
    t = t_of(n)
    l, r = np.zeros(n), np.zeros(n)
    for note in notes:
        f = midi(note)
        l += saw(f, t, -0.004) + saw(f, t, 0.003)
        r += saw(f, t, 0.004) + saw(f, t, -0.002)
    e = env(n, 0.35, 0.3, 0.8, 0.6)
    return np.stack([one_pole(l, bright) * e, one_pole(r, bright) * e], 1) * 0.055


def pluck(note, dur=0.3):
    n = int(dur * SR)
    t = t_of(n)
    f = midi(note)
    x = saw(f, t) * 0.45 + np.sin(2 * np.pi * f * t) * 0.55
    return one_pole(x, 500 + 3800 * np.exp(-t * 16)) * np.exp(-t * 8) * 0.3


def bell(note, dur=2.0):
    n = int(dur * SR)
    t = t_of(n)
    f = midi(note)
    x = (np.sin(2 * np.pi * f * t) + 0.4 * np.sin(2 * np.pi * f * 2.76 * t) * np.exp(-t * 4)
         + 0.18 * np.sin(2 * np.pi * f * 5.4 * t) * np.exp(-t * 8))
    return x * np.exp(-t * 2.4) * 0.28


# D major: I V vi IV, voiced around middle C (root, chord)
KEY = 2
PROG = [(48, [55, 60, 64, 67]), (43, [55, 59, 62, 67]), (45, [57, 60, 64, 69]), (41, [57, 60, 65, 69])]
PROG = [(r + KEY, [c + KEY for c in ch]) for r, ch in PROG]
ARP = [0, 2, 1, 3, 2, 1, 3, 2]
BELL_LINE = [(0, 3), (1.5, 2), (3, 1), (4.5, 2), (6, 3), (7, 2)]  # (beat, chord index)


def build_music():
    n = int(TOTAL * SR)
    mix = np.zeros((n, 2))

    def add(sig, at, gain=1.0, pan=0.0):
        sig = np.asarray(sig)
        if sig.ndim == 1:
            sig = np.stack([sig * (1 - max(pan, 0)), sig * (1 + min(pan, 0))], 1)
        i = int(round(at * SR))
        if i >= n:
            return
        j = min(n, i + len(sig))
        mix[i:j] += sig[: j - i] * gain

    nbars = int(round(TOTAL / BAR))
    for b in range(nbars):
        t0 = b * BAR
        root, chord = PROG[b % len(PROG)]
        intro, top, outro = t0 < INTRO, FIRST <= t0 < OUTRO, t0 >= OUTRO
        last_bars = t0 >= TOTAL - 2 * BAR

        # pad: darker in the intro, opens up for #1
        bright = 700 if intro else (1700 if top else 1200)
        if not last_bars or b == nbars - 2:
            add(pad(chord, BAR + 0.6, bright), t0, 1.3 if intro or outro else 1.0)

        # arpeggio in eighths; sparser in the outro
        for k in range(8):
            if outro and k % 2:
                continue
            if FIRST - BEAT <= t0 + k * BEAT / 2 < FIRST:
                continue  # the silent beat before #1
            note = chord[ARP[k] % 4] + 12
            add(pluck(note), t0 + k * BEAT / 2, 0.75 if intro else 0.9, pan=0.35 if k % 2 else -0.35)

        for beat in range(4):
            at = t0 + beat * BEAT
            if FIRST - BEAT <= at < FIRST:
                continue
            if intro:
                if b == 1:
                    add(hat(), at + BEAT / 2, 0.5, pan=0.2)
                continue
            if outro and t0 >= OUTRO + BAR:
                continue  # drums stop for the last two bars
            # half-time: kick on 1 (and the "and" of 2), snare on 3, eighth hats
            if beat == 0:
                add(kick(1.05), at, 0.95)
                add(bass(root, BEAT * 1.8), at, 0.85)
            if beat == 1 and not outro:
                add(kick(0.75), at + BEAT / 2, 0.6)
            if beat == 2:
                add(snare(), at, 0.85)
                if top:
                    add(clap(), at, 0.6)
                add(bass(root + (7 if b % 2 else 0), BEAT * 1.4), at + BEAT / 2, 0.65)
            add(hat(), at, 0.32, pan=0.2)
            add(hat(), at + BEAT / 2, 0.5, pan=-0.2)
            # a 16th roll into each launch's first bar
            if (b % 4 == 0) and beat == 3 and not outro:
                for s in range(4):
                    add(hat(), at + s * BEAT / 4, 0.25 + 0.1 * s, pan=0.1)

        if top or (outro and t0 < OUTRO + BAR):
            for beat, idx in BELL_LINE:
                if beat < 4:
                    add(bell(chord[idx] + 24, 1.6), t0 + beat * BEAT, 0.45 if top else 0.35,
                        pan=0.15 if idx % 2 else -0.15)

    # a final chord ring for the leaderboard
    root, chord = PROG[0]
    for i, note in enumerate(chord[1:]):
        add(bell(note + 12, 3.0), OUTRO + BAR + i * BEAT / 2, 0.6, pan=(i - 1) * 0.2)

    mix = np.tanh(mix * 1.15)
    fade = int(2.0 * SR)
    mix[-fade:] *= np.linspace(1, 0, fade)[:, None]
    write("music.wav", mix)


def build_sfx():
    # whoosh: band-passed noise sweep peaking at 0.25 s, the middle of the 0.5 s wipe
    n = int(0.55 * SR)
    t = t_of(n)
    noise = rng.standard_normal(n)
    shape = np.exp(-((t - 0.25) / 0.11) ** 2)
    sweep = 400 + 6000 * shape
    w = (one_pole(noise, sweep) - one_pole(noise, sweep * 0.3)) * shape
    write("whoosh.wav", w)

    # tick: soft counter tick
    n = int(0.03 * SR)
    t = t_of(n)
    write("tick.wav", np.sin(2 * np.pi * 2900 * t) * np.exp(-t * 220))

    # pop: short blip for the upvote press
    n = int(0.14 * SR)
    t = t_of(n)
    f = 700 + 700 * np.exp(-t * 50)
    write("pop.wav", np.sin(2 * np.pi * np.cumsum(f) / SR) * np.exp(-t * 38))

    # riser into #1, 1.5 s, peaks at its end
    n = int(1.5 * SR)
    t = t_of(n)
    r = one_pole(rng.standard_normal(n), 300 + 9000 * (t / 1.5) ** 2) * (t / 1.5) ** 2
    f = 220 + 880 * (t / 1.5) ** 2
    r += np.sin(2 * np.pi * np.cumsum(f) / SR) * (t / 1.5) ** 3 * 0.3
    write("riser.wav", r)

    # impact for #1: sub drop + noise burst + tail
    n = int(2.2 * SR)
    t = t_of(n)
    f = 32 + 90 * np.exp(-t * 6)
    sub = np.sin(2 * np.pi * np.cumsum(f) / SR) * np.exp(-t * 2.4)
    burst = one_pole(rng.standard_normal(n), 2500) * np.exp(-t * 6) * 0.5
    tails = [one_pole(rng.standard_normal(n), 1100) * np.exp(-t * 2.6) * 0.14 for _ in range(2)]
    write("impact.wav", np.stack([sub + burst + tails[0], sub + burst + tails[1]], 1))


if __name__ == "__main__":
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--seed", type=int, default=1128, help="noise seed for drums and effects")
    rng = np.random.default_rng(ap.parse_args().seed)
    os.makedirs(OUT, exist_ok=True)
    build_sfx()
    build_music()
