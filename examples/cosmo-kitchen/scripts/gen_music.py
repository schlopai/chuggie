#!/usr/bin/env python3
"""Cosmo Kitchen deck pack — each song has its own harmony, rhythm, and lead DNA.

Spoon (C-E-G) and burn (m2 down) appear as rare signatures, not loop fillers.
Channel caps: 2 pulse / 1 wave / 1 noise / 2 PCM. INT songs use layer 0..3.
"""
from __future__ import annotations

from pathlib import Path

OUT = Path(__file__).resolve().parents[1] / "assets" / "music"

Event = tuple[int, float, float, int]  # midi, start, dur, vel


def N(m: int, s: float, d: float, v: int = 100) -> Event:
    return (m, s, d, v)


def emit(lines: list[str], ev: list[Event]) -> None:
    for m, s, d, v in sorted(ev, key=lambda e: (e[1], e[0])):
        lines.append(f"  note {m} {s:g} {d:g} v {v}")


def shift(ev: list[Event], beats: float) -> list[Event]:
    return [(m, s + beats, d, v) for m, s, d, v in ev]


def repeat_section(ev: list[Event], bars: int, times: int) -> list[Event]:
    out: list[Event] = []
    for i in range(times):
        out.extend(shift(ev, i * bars * 4.0))
    return out


def bass_from_roots(roots: list[int], style: str) -> list[Event]:
    """roots: one MIDI root per bar (length = bars)."""
    ev: list[Event] = []
    for bi, r in enumerate(roots):
        t = bi * 4.0
        # Keep wave bass in hardware-safe range (~C2..C3)
        b = r
        while b > 48:
            b -= 12
        while b < 31:
            b += 12
        if style == "pedal":
            ev.append(N(b, t, 4.0, 110))
        elif style == "half":
            ev += [N(b, t, 2.0, 115), N(b, t + 2.0, 2.0, 95)]
        elif style == "drive":
            for i in range(8):
                ev.append(N(b if i % 4 != 3 else min(b + 2, 48), t + i * 0.5, 0.45, 120 if i % 2 == 0 else 90))
        elif style == "syncop":
            ev += [
                N(b, t, 0.75, 120),
                N(b, t + 1.0, 0.5, 100),
                N(b + 5 if b + 5 <= 48 else b - 7, t + 2.0, 0.75, 115),
                N(b, t + 3.0, 0.9, 105),
            ]
        elif style == "waltz":  # 3+1 feel inside 4/4
            ev += [N(b, t, 1.5, 115), N(b, t + 1.5, 1.5, 95), N(b + 7 if b + 7 <= 48 else b, t + 3.0, 0.9, 100)]
        elif style == "broken":
            third = b + 3 if (b + 3) <= 48 else b + 2
            fifth = b + 7 if (b + 7) <= 48 else b + 5
            ev += [N(b, t, 1.0, 120), N(fifth, t + 1.0, 1.0, 100), N(third, t + 2.0, 1.0, 105), N(b, t + 3.0, 1.0, 110)]
        else:  # four-on-root with octave hop
            for i in range(4):
                ev.append(N(b if i != 2 else max(b - 12, 31) + 12, t + i, 0.9, 115 if i == 0 else 95))
    return ev


def pad_from_roots(roots: list[int], hold: float = 4.0, voicing: str = "root") -> list[Event]:
    ev: list[Event] = []
    t = 0.0
    for i, r in enumerate(roots):
        p = r
        while p < 48:
            p += 12
        while p > 67:
            p -= 12
        if voicing == "fifth":
            p = p + 7 if p + 7 <= 72 else p - 5
        elif voicing == "third":
            p = p + 4 if p + 4 <= 72 else p + 3
        elif voicing == "cluster":
            p = p + 2
        ev.append(N(p, t, hold - 0.05, 60 + (i % 5) * 5))
        t += hold
    return ev


def kick(bars: int, pattern: str) -> list[Event]:
    ev: list[Event] = []
    for b in range(bars):
        t = b * 4.0
        if pattern == "four":
            for i in range(4):
                ev.append(N(34, t + i, 0.12, 120 if i % 2 == 0 else 105))
        elif pattern == "break":
            ev += [N(34, t, 0.12, 127), N(34, t + 1.5, 0.1, 100), N(34, t + 2, 0.12, 120), N(34, t + 3.25, 0.1, 95)]
        elif pattern == "sparse":
            ev += [N(34, t, 0.14, 100), N(34, t + 2.5, 0.12, 90)]
        elif pattern == "disco":
            for i in range(4):
                ev += [N(34, t + i, 0.1, 125), N(34, t + i + 0.5, 0.08, 85)]
        elif pattern == "half":
            ev += [N(34, t, 0.14, 115), N(34, t + 2, 0.14, 110)]
        elif pattern == "shuffle":
            ev += [N(34, t, 0.12, 120), N(34, t + 0.75, 0.1, 90), N(34, t + 2, 0.12, 115), N(34, t + 2.75, 0.1, 90)]
        elif pattern == "dembow":
            ev += [N(34, t, 0.12, 127), N(34, t + 1.5, 0.12, 120), N(34, t + 2, 0.1, 100), N(34, t + 3.5, 0.12, 115)]
        else:  # none-ish
            if b % 2 == 0:
                ev.append(N(34, t, 0.14, 90))
    return ev


def hats_pcm(bars: int, pattern: str) -> list[Event]:
    ev: list[Event] = []
    for b in range(bars):
        t = b * 4.0
        if pattern == "offbeat":
            for i in range(4):
                ev.append(N(80, t + i + 0.5, 0.07, 70))
        elif pattern == "eighth":
            for i in range(8):
                ev.append(N(80, t + i * 0.5, 0.06, 85 if i % 2 == 0 else 55))
        elif pattern == "drip":
            ev += [N(84, t + 0.75, 0.1, 50), N(79, t + 2.75, 0.12, 45)]
        elif pattern == "clave":
            for o in (0.0, 1.5, 3.0):
                ev.append(N(82, t + o, 0.08, 75))
        elif pattern == "swing":
            for i in range(4):
                ev += [N(80, t + i, 0.05, 60), N(80, t + i + 0.66, 0.05, 80)]
        elif pattern == "16th":
            for i in range(16):
                ev.append(N(80, t + i * 0.25, 0.05, 90 if i % 4 == 0 else 50))
        elif pattern == "none":
            pass
        else:
            ev.append(N(80, t + 3.5, 0.08, 55))
    return ev


def hats_noise(bars: int, pattern: str) -> list[Event]:
    # reuse pcm patterns at noise pitch
    return [(60, s, d, v) for _, s, d, v in hats_pcm(bars, pattern)]


def write_header(lines: list[str], title: str, comment: str, bpm: int) -> None:
    lines += [f"# {title}", f"# {comment}", "deck 1", f"bpm {bpm}", ""]


def track_pulse(lines: list[str], name: str, tid: str, bars: int, duty: str, vol: int,
                ev: list[Event], *, layer: int | None = None, vib=None, arp=None, env="constant") -> None:
    lay = f" layer {layer}" if layer is not None else ""
    lines.append(f"track {name} id {tid} gen gameBoyDmg{lay} * {bars}")
    lines.append(f"  gen type pulse duty {duty} env_mode {env} vol {vol}")
    if vib:
        lines.append(f"  gen vib_rate {vib[0]} vib_amt {vib[1]}")
    if arp:
        lines.append(f"  gen arp_rate {arp[0]} arp_semis {arp[1]}")
    emit(lines, ev)
    lines.append("")


def track_wave(lines: list[str], bars: int, shape: str, ev: list[Event], *, layer: int | None = None) -> None:
    lay = f" layer {layer}" if layer is not None else ""
    lines.append(f"track Bass id bass gen gameBoyDmg{lay} * {bars}")
    lines.append(f"  gen type wave wave_shape {shape} vol 14")
    emit(lines, ev)
    lines.append("")


def track_noise(lines: list[str], bars: int, ev: list[Event], *, layer: int | None = None, vol: int = 6) -> None:
    lay = f" layer {layer}" if layer is not None else ""
    lines.append(f"track Hats id hats gen gameBoyDmg{lay} * {bars}")
    lines.append(f"  gen type noise noise_mode short vol {vol} env_mode step env_step 2")
    emit(lines, ev)
    lines.append("")


def track_kick(lines: list[str], bars: int, ev: list[Event], *, layer: int | None = None, drop: int = -10) -> None:
    lay = f" layer {layer}" if layer is not None else ""
    lines.append(f"track Kick id kick gen gbaDirectSound{lay} * {bars}")
    lines.append(f"  gen waveform pulse duty 50 vol 14 bitcrush true pitch_drop {drop} pitch_dec 0.06")
    lines.append("  adsr a 0 d 0.1 s 0 r 0.03")
    emit(lines, ev)
    lines.append("")


def track_pcm_hat(lines: list[str], bars: int, ev: list[Event], wave: str = "triangle", vol: int = 7) -> None:
    lines.append(f"track Hats id hats gen gbaDirectSound * {bars}")
    lines.append(f"  gen waveform {wave} vol {vol} bitcrush true")
    lines.append("  adsr a 0 d 0.04 s 0 r 0.02")
    emit(lines, ev)
    lines.append("")


def track_stab(lines: list[str], bars: int, ev: list[Event], *, layer: int = 3, wave: str = "sawtooth") -> None:
    lines.append(f"track Stab id stab gen gbaDirectSound layer {layer} * {bars}")
    lines.append(f"  gen waveform {wave} vol 11 bitcrush true")
    lines.append("  adsr a 0 d 0.08 s 0 r 0.05")
    emit(lines, ev)
    lines.append("")


def save(name: str, lines: list[str]) -> None:
    path = OUT / name
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"wrote {name}")


# ── Handcrafted lead cells (one bar = 4 beats unless noted) ──────────────────

def spoon_once(t: float, root: int = 60) -> list[Event]:
    return [N(root, t, 0.2, 110), N(root + 4, t + 0.2, 0.2, 115), N(root + 7, t + 0.4, 0.35, 120)]


def burn_once(t: float, top: int = 67) -> list[Event]:
    return [N(top, t, 0.25, 110), N(top - 1, t + 0.3, 0.45, 100)]


# ============================================================================
# Songs
# ============================================================================

def song_title() -> None:
    # Lydian-ish bright: Cmaj7 - D - Em - G, then A - G - F - G
    bars = 16
    roots = [48, 50, 52, 55, 57, 55, 53, 55, 48, 50, 52, 55, 60, 55, 53, 48]
    lead_a = [
        N(72, 0, 0.75, 110), N(71, 0.75, 0.25, 100), N(69, 1, 0.5, 105), N(67, 1.5, 0.5, 100),
        N(69, 2, 1.0, 115), N(72, 3, 0.5, 110), N(74, 3.5, 0.5, 120),
    ]
    lead_b = [
        N(76, 0, 0.5, 120), N(74, 0.5, 0.5, 110), N(72, 1, 1.0, 115),
        N(69, 2, 0.5, 105), N(67, 2.5, 0.5, 100), N(69, 3, 1.0, 110),
    ]
    lead_c = [
        N(67, 0, 2.0, 90), N(69, 2.5, 0.5, 100), N(72, 3.25, 0.7, 115),
    ]
    lead = (
        repeat_section(lead_a, 1, 4)
        + shift(repeat_section(lead_b, 1, 4), 16)
        + shift(repeat_section(lead_c, 1, 4), 32)
        + shift(repeat_section(lead_a, 1, 3), 48)
        + shift(spoon_once(0, 72), 60)
    )
    lines: list[str] = []
    write_header(lines, "Neon Diner - title", "Lydian lounge fanfare; long held tops, rare spoon tag.", 104)
    track_pulse(lines, "Pad", "pad", bars, "12_5", 4, pad_from_roots(roots, voicing="third"), vib=(2, 14))
    track_pulse(lines, "Lead", "lead", bars, "50", 11, lead, vib=(3, 16))
    track_wave(lines, bars, "sine", bass_from_roots(roots, "half"))
    track_kick(lines, bars, kick(bars, "sparse"), drop=-8)
    track_pcm_hat(lines, bars, hats_pcm(bars, "swing"), "sine", 6)
    save("title-neon-diner.deck", lines)


def song_hub() -> None:
    # Funk: Dm7 - G7 - Cmaj7 - A7 (ii-V-I-VI) cycling, then Bb - F - C - G
    bars = 16
    roots = [41, 43, 36, 33, 41, 43, 36, 33, 46, 41, 36, 43, 41, 43, 36, 36]
    # Opening MUST be audible midrange for verify — strong C5 punch then funk riff
    riff = [
        N(60, 0, 0.25, 120), N(63, 0.25, 0.25, 110), N(65, 0.5, 0.25, 115), N(60, 0.75, 0.25, 100),
        N(67, 1.0, 0.5, 125), N(65, 1.5, 0.25, 110), N(63, 1.75, 0.25, 105),
        N(60, 2.0, 0.25, 115), N(58, 2.5, 0.25, 100), N(60, 3.0, 0.5, 120), N(63, 3.5, 0.5, 110),
    ]
    bridge = [
        N(70, 0, 0.5, 120), N(67, 0.5, 0.5, 110), N(65, 1, 0.5, 115), N(63, 1.5, 0.5, 105),
        N(65, 2, 0.25, 110), N(67, 2.25, 0.25, 115), N(70, 2.5, 0.5, 125), N(72, 3.25, 0.7, 120),
    ]
    lead = repeat_section(riff, 1, 8) + shift(repeat_section(bridge, 1, 4), 32) + shift(repeat_section(riff, 1, 4), 48)
    lead += shift(spoon_once(0, 60), 48)  # spoon once at bar 12
    lines: list[str] = []
    write_header(lines, "Starport Market - hub", "ii-V funk. Opens on C4 riff for verify. Spoon once at bar 12.", 118)
    track_pulse(lines, "Pad", "pad", bars, "25", 5, pad_from_roots(roots, voicing="fifth"), vib=(5, 6))
    track_pulse(lines, "Lead", "lead", bars, "50", 13, lead, vib=(4, 8))
    track_wave(lines, bars, "saw", bass_from_roots(roots, "syncop"))
    track_kick(lines, bars, kick(bars, "disco"), drop=-12)
    track_pcm_hat(lines, bars, hats_pcm(bars, "clave"), "triangle", 8)
    save("hub-starport-market.deck", lines)


def song_menu() -> None:
    # Soft ambient: open fifths drifting Am - F - C - Em, whole notes
    bars = 12
    roots = [45, 41, 36, 40, 45, 43, 41, 40, 45, 41, 36, 45]
    lead = []
    for bi, r in enumerate(roots):
        t = bi * 4.0
        if bi % 3 == 0:
            lead.append(N(r + 24, t + 1.0, 2.5, 70))
        elif bi % 3 == 1:
            lead += [N(r + 19, t + 2.0, 1.5, 65)]
        # mostly silence — menus need air
    lines: list[str] = []
    write_header(lines, "Clipboard - menus", "Sparse ambient fifths. Almost no drums.", 84)
    track_pulse(lines, "Pad", "pad", bars, "12_5", 4, pad_from_roots(roots, voicing="fifth"), vib=(1, 10))
    track_pulse(lines, "Lead", "lead", bars, "25", 7, lead, vib=(2, 12))
    track_wave(lines, bars, "sine", bass_from_roots(roots, "pedal"))
    track_kick(lines, bars, kick(bars, "none"), drop=-6)
    track_pcm_hat(lines, bars, hats_pcm(bars, "drip"), "sine", 4)
    save("menu-clipboard.deck", lines)


def song_shop() -> None:
    # Bright jingle: G - Em - C - D, then G - A - D - G. Staccato lead.
    bars = 12
    roots = [43, 40, 36, 38, 43, 45, 38, 43, 43, 40, 38, 43]
    cell = [
        N(67, 0, 0.2, 120), N(71, 0.25, 0.2, 115), N(74, 0.5, 0.2, 125), N(71, 0.75, 0.2, 110),
        N(67, 1.25, 0.2, 115), N(64, 1.5, 0.2, 105), N(67, 1.75, 0.2, 110),
        N(71, 2.25, 0.35, 120), N(74, 2.75, 0.2, 115), N(79, 3.25, 0.6, 127),
    ]
    lead = repeat_section(cell, 1, 8) + shift([
        N(76, 0, 0.5, 120), N(74, 0.5, 0.5, 110), N(71, 1, 0.5, 115), N(67, 1.5, 0.5, 105),
        N(71, 2, 0.5, 120), N(74, 2.5, 0.5, 115), N(79, 3, 0.9, 127),
    ], 32) + shift(repeat_section(cell, 1, 3), 36)
    lines: list[str] = []
    write_header(lines, "Vendomat - shop", "Staccato vendor jingle in G. Order-bell friendly tops.", 128)
    track_pulse(lines, "Pad", "pad", bars, "50", 4, pad_from_roots(roots, voicing="root"), vib=(6, 4))
    track_pulse(lines, "Lead", "lead", bars, "50", 12, lead)
    track_wave(lines, bars, "square", bass_from_roots(roots, "four"))
    track_kick(lines, bars, kick(bars, "four"), drop=-10)
    track_pcm_hat(lines, bars, hats_pcm(bars, "offbeat"), "triangle", 7)
    save("shop-vendomat.deck", lines)


def song_hyperspace() -> None:
    # Floating whole-tone-ish: pedal Eb with planing major thirds up
    bars = 16
    roots = [39] * 8 + [41, 41, 43, 43, 39, 38, 39, 39]
    lead = []
    for bi in range(bars):
        t = bi * 4.0
        base = 63 + (bi % 5)
        if bi < 8:
            lead += [N(base, t, 3.5, 85), N(base + 4, t + 1.5, 2.0, 70)]
        else:
            lead += [N(base + 2, t, 1.0, 95), N(base + 6, t + 1.5, 1.0, 90), N(base + 4, t + 3.0, 0.9, 100)]
    lines: list[str] = []
    write_header(lines, "Hyperspace Cruise - overworld", "Pedal drone + planing thirds. Slow glide.", 100)
    track_pulse(lines, "Pad", "pad", bars, "25", 6, pad_from_roots(roots, voicing="cluster"), vib=(2, 18))
    track_pulse(lines, "Lead", "lead", bars, "50", 10, lead, vib=(3, 20))
    track_wave(lines, bars, "sine", bass_from_roots(roots, "pedal"))
    track_kick(lines, bars, kick(bars, "half"), drop=-7)
    track_pcm_hat(lines, bars, hats_pcm(bars, "drip"), "sine", 5)
    save("zone-hyperspace-cruise.deck", lines)


def song_agri() -> None:
    # Pastoral pentatonic F: F - Bb - C - F / Dm - Bb - C - F
    bars = 16
    roots = [41, 46, 36, 41, 38, 46, 36, 41, 41, 46, 43, 41, 38, 46, 36, 41]
    cell = [
        N(65, 0, 0.5, 100), N(69, 0.5, 0.5, 105), N(72, 1, 1.0, 115),
        N(69, 2.25, 0.5, 100), N(65, 2.75, 0.5, 95), N(60, 3.5, 0.5, 105),
    ]
    call = [
        N(72, 0, 0.75, 110), N(77, 1, 0.75, 120), N(72, 2, 0.5, 105), N(69, 2.75, 1.0, 100),
    ]
    lead = repeat_section(cell, 1, 8) + shift(repeat_section(call, 1, 4), 32) + shift(repeat_section(cell, 1, 4), 48)
    lines: list[str] = []
    write_header(lines, "Agri-Dome - farm", "F pentatonic pastoral. Warm sine bass.", 96)
    track_pulse(lines, "Pad", "pad", bars, "25", 5, pad_from_roots(roots, voicing="third"), vib=(3, 10))
    track_pulse(lines, "Lead", "lead", bars, "50", 11, lead, vib=(4, 12))
    track_wave(lines, bars, "sine", bass_from_roots(roots, "half"))
    track_kick(lines, bars, kick(bars, "sparse"), drop=-9)
    track_pcm_hat(lines, bars, hats_pcm(bars, "offbeat"), "triangle", 5)
    save("zone-agri-dome.deck", lines)


def song_night() -> None:
    # Noir: Cm - Ab - Eb - Bb, then Fm - Ab - G - Cm. Syncopated sparse lead.
    bars = 16
    roots = [36, 44, 39, 34, 36, 44, 39, 34, 41, 44, 43, 36, 41, 44, 43, 36]
    cell = [
        N(63, 0.5, 0.5, 100), N(66, 1.5, 0.75, 110), N(63, 2.5, 0.25, 95),
        N(58, 3.25, 0.7, 105),
    ]
    rise = [
        N(58, 0, 0.5, 100), N(63, 0.75, 0.5, 110), N(66, 1.5, 0.5, 115), N(70, 2.25, 0.75, 125),
        N(66, 3.25, 0.7, 110),
    ]
    lead = repeat_section(cell, 1, 8) + shift(repeat_section(rise, 1, 4), 32) + shift(repeat_section(cell, 1, 4), 48)
    lead += shift(burn_once(0, 70), 56)
    lines: list[str] = []
    write_header(lines, "Neon Night Market - sidequest hub", "Noir Cm. Offbeat entries. Burn tag near end.", 102)
    track_pulse(lines, "Pad", "pad", bars, "12_5", 5, pad_from_roots(roots, voicing="root"), vib=(2, 14))
    track_pulse(lines, "Lead", "lead", bars, "12_5", 12, lead, vib=(3, 14))
    track_wave(lines, bars, "saw", bass_from_roots(roots, "broken"))
    track_kick(lines, bars, kick(bars, "shuffle"), drop=-11)
    track_pcm_hat(lines, bars, hats_pcm(bars, "drip"), "triangle", 6)
    save("zone-neon-night-market.deck", lines)


def song_liner() -> None:
    # Smarmy waltz-leaning: A - F#m - D - E, then C#m - D - E - A
    bars = 16
    roots = [45, 42, 38, 40, 45, 42, 38, 40, 37, 38, 40, 45, 37, 38, 40, 45]
    cell = [
        N(69, 0, 1.0, 105), N(73, 1.0, 0.5, 110), N(76, 1.5, 0.5, 115),
        N(73, 2.25, 0.75, 105), N(69, 3.25, 0.7, 100),
    ]
    flourish = [
        N(81, 0, 0.35, 120), N(76, 0.4, 0.35, 115), N(73, 0.8, 0.35, 110), N(69, 1.2, 0.35, 105),
        N(73, 1.8, 0.5, 115), N(76, 2.5, 0.5, 120), N(81, 3.25, 0.7, 125),
    ]
    lead = repeat_section(cell, 1, 8) + shift(repeat_section(flourish, 1, 4), 32) + shift(repeat_section(cell, 1, 4), 48)
    lines: list[str] = []
    write_header(lines, "Cruise Liner - luxury town", "Waltz-leaning A major. Smarmy flourishes.", 90)
    track_pulse(lines, "Pad", "pad", bars, "12_5", 5, pad_from_roots(roots, voicing="third"), vib=(2, 8))
    track_pulse(lines, "Lead", "lead", bars, "50", 11, lead, vib=(2, 10))
    track_wave(lines, bars, "sine", bass_from_roots(roots, "waltz"))
    track_kick(lines, bars, kick(bars, "sparse"), drop=-8)
    track_pcm_hat(lines, bars, hats_pcm(bars, "swing"), "sine", 6)
    save("zone-cruise-liner.deck", lines)


def song_asteroid() -> None:
    # NEW: rocky 5-chord vamp Bbm - Gb - Ab - Db - Eb
    bars = 16
    roots = [34, 42, 44, 37, 39, 34, 42, 44, 37, 39, 34, 42, 44, 37, 39, 34]
    cell = [
        N(58, 0, 0.25, 120), N(61, 0.25, 0.25, 115), N(63, 0.5, 0.5, 125),
        N(58, 1.25, 0.25, 110), N(56, 1.5, 0.25, 105), N(58, 1.75, 0.25, 110),
        N(63, 2.25, 0.5, 120), N(66, 2.9, 0.35, 125), N(63, 3.4, 0.5, 115),
    ]
    lead = repeat_section(cell, 1, 12) + shift([
        N(70, 0, 0.5, 127), N(66, 0.5, 0.5, 120), N(63, 1, 0.5, 115), N(61, 1.5, 0.5, 110),
        N(58, 2, 1.0, 120), N(56, 3.25, 0.7, 105),
    ], 48) + shift(repeat_section(cell, 1, 3), 52)
    lines: list[str] = []
    write_header(lines, "Asteroid Diner - rocky outpost", "Bbm rock vamp. Punchy staccato lead.", 126)
    track_pulse(lines, "Pad", "pad", bars, "25", 5, pad_from_roots(roots, voicing="root"))
    track_pulse(lines, "Lead", "lead", bars, "25", 13, lead)
    track_wave(lines, bars, "square", bass_from_roots(roots, "drive"))
    track_kick(lines, bars, kick(bars, "dembow"), drop=-14)
    track_pcm_hat(lines, bars, hats_pcm(bars, "eighth"), "triangle", 8)
    save("zone-asteroid-diner.deck", lines)


def song_icebox() -> None:
    # Frozen: slow Dorian D: Dm - C - Bb - A, sparse dripping melody
    bars = 16
    roots = [38, 36, 34, 33, 38, 36, 34, 33, 38, 41, 36, 33, 38, 34, 33, 38]
    lead = []
    for bi in range(bars):
        t = bi * 4.0
        if bi % 4 == 0:
            lead += [N(62, t + 1.5, 1.5, 80)]
        elif bi % 4 == 2:
            lead += [N(57, t + 0.5, 0.5, 70), N(60, t + 2.5, 1.2, 85)]
        elif bi == 15:
            lead += burn_once(t + 1.0, 62)
    lines: list[str] = []
    write_header(lines, "Icebox - freezer dungeon", "Sparse Dorian. Drips. Almost hollow.", 78)
    track_pulse(lines, "Pad", "pad", bars, "12_5", 3, pad_from_roots(roots, voicing="fifth"), vib=(1, 6))
    track_pulse(lines, "Lead", "lead", bars, "25", 9, lead, vib=(1, 8))
    track_wave(lines, bars, "sine", bass_from_roots(roots, "pedal"))
    track_kick(lines, bars, kick(bars, "none"), drop=-5)
    track_pcm_hat(lines, bars, hats_pcm(bars, "drip"), "sine", 5)
    save("dungeon-icebox.deck", lines)


def song_spice() -> None:
    # Phrygian E: Em - F - Em - G / Am - F - Em - B
    bars = 16
    roots = [40, 41, 40, 43, 40, 41, 40, 43, 45, 41, 40, 35, 45, 41, 40, 40]
    cell = [
        N(64, 0, 0.25, 120), N(65, 0.25, 0.25, 115), N(64, 0.5, 0.25, 120), N(67, 0.75, 0.25, 125),
        N(64, 1.25, 0.25, 115), N(60, 1.5, 0.25, 110), N(64, 1.75, 0.25, 115),
        N(65, 2.25, 0.5, 120), N(67, 2.9, 0.35, 125), N(72, 3.4, 0.5, 127),
    ]
    lead = repeat_section(cell, 1, 16)
    lines: list[str] = []
    write_header(lines, "Spice Mines - volcanic", "Phrygian hammer-ons + arp. Hot drums.", 140)
    track_pulse(lines, "Pad", "pad", bars, "25", 5, pad_from_roots(roots, voicing="root"), vib=(5, 6))
    track_pulse(lines, "Lead", "lead", bars, "12_5", 13, lead, arp=(12, 5), vib=(5, 8))
    track_wave(lines, bars, "saw", bass_from_roots(roots, "drive"))
    track_noise(lines, bars, hats_noise(bars, "16th"), vol=7)
    track_kick(lines, bars, kick(bars, "break"), drop=-14)
    save("dungeon-spice-mines.deck", lines)


def song_sugar() -> None:
    # Twinkly Lydian C: C - D - G - C / Am - D - G - C
    bars = 16
    roots = [36, 38, 43, 36, 33, 38, 43, 36, 36, 38, 43, 36, 33, 38, 43, 48]
    cell = [
        N(72, 0, 0.25, 110), N(74, 0.25, 0.25, 115), N(76, 0.5, 0.25, 120), N(79, 0.75, 0.25, 125),
        N(76, 1.25, 0.25, 115), N(74, 1.5, 0.25, 110), N(72, 1.75, 0.25, 105),
        N(79, 2.25, 0.5, 120), N(84, 2.9, 0.4, 125), N(79, 3.5, 0.45, 115),
    ]
    lead = repeat_section(cell, 1, 12) + shift(spoon_once(0, 72), 48) + shift(repeat_section(cell, 1, 3), 50)
    lines: list[str] = []
    write_header(lines, "Sugar Caverns - crystal", "Lydian sparkle runs. Glass PCM.", 122)
    track_pulse(lines, "Pad", "pad", bars, "12_5", 4, pad_from_roots(roots, voicing="third"), vib=(6, 14))
    track_pulse(lines, "Lead", "lead", bars, "50", 12, lead, vib=(6, 12))
    track_wave(lines, bars, "sine", bass_from_roots(roots, "half"))
    track_kick(lines, bars, kick(bars, "four"), drop=-8)
    track_pcm_hat(lines, bars, hats_pcm(bars, "eighth"), "sine", 8)
    save("dungeon-sugar-caverns.deck", lines)


def song_galley() -> None:
    # Industrial: chromatic bass crawl + tritones in lead
    bars = 16
    roots = [36, 37, 38, 37, 36, 35, 34, 35, 36, 38, 40, 38, 36, 34, 33, 36]
    lead = []
    for bi in range(bars):
        t = bi * 4.0
        if bi % 2 == 0:
            lead += [N(54, t + 0.5, 0.5, 100), N(60, t + 1.5, 0.5, 110)]  # tritone
        else:
            lead += [N(58, t + 1.0, 1.0, 90), N(54, t + 3.0, 0.8, 85)]
    lines: list[str] = []
    write_header(lines, "Derelict Galley - abandoned ship", "Chromatic crawl + tritones. Irregular hats.", 94)
    track_pulse(lines, "Pad", "pad", bars, "25", 4, pad_from_roots(roots, voicing="cluster"), vib=(1, 4))
    track_pulse(lines, "Lead", "lead", bars, "25", 11, lead)
    track_wave(lines, bars, "square", bass_from_roots(roots, "drive"))
    track_noise(lines, bars, hats_noise(bars, "clave"), vol=8)
    track_kick(lines, bars, kick(bars, "break"), drop=-12)
    save("dungeon-derelict-galley.deck", lines)


def song_compactor() -> None:
    # NEW dungeon: odd accents, Locrian-ish
    bars = 12
    roots = [35, 36, 35, 38, 35, 33, 35, 36, 38, 35, 33, 35]
    cell = [
        N(59, 0, 0.5, 115), N(58, 0.75, 0.25, 110), N(59, 1.25, 0.25, 115),
        N(62, 1.75, 0.5, 120), N(59, 2.5, 0.5, 110), N(55, 3.25, 0.7, 105),
    ]
    lead = repeat_section(cell, 1, 12)
    lines: list[str] = []
    write_header(lines, "Waste Compactor - dungeon", "Locrian grind. Crushing kicks.", 110)
    track_pulse(lines, "Pad", "pad", bars, "25", 5, pad_from_roots(roots, voicing="root"))
    track_pulse(lines, "Lead", "lead", bars, "12_5", 12, lead)
    track_wave(lines, bars, "saw", bass_from_roots(roots, "syncop"))
    track_noise(lines, bars, hats_noise(bars, "eighth"), vol=7)
    track_kick(lines, bars, kick(bars, "dembow"), drop=-15)
    save("dungeon-waste-compactor.deck", lines)


def song_skirmish() -> None:
    # INT battle: Gm - Eb - F - Dm / Gm - Bb - F - D
    bars = 16
    roots = [43, 39, 41, 38, 43, 46, 41, 38, 43, 39, 41, 38, 43, 46, 41, 43]
    bed_pad = pad_from_roots(roots, voicing="fifth")
    bass = bass_from_roots(roots, "drive")
    lead_cell = [
        N(67, 0, 0.25, 120), N(70, 0.25, 0.25, 115), N(72, 0.5, 0.25, 125), N(70, 0.75, 0.25, 115),
        N(67, 1.25, 0.25, 120), N(65, 1.5, 0.25, 110), N(67, 1.75, 0.25, 115),
        N(70, 2.25, 0.5, 125), N(74, 2.9, 0.35, 127), N(72, 3.4, 0.5, 120),
    ]
    lead = repeat_section(lead_cell, 1, 16)
    stabs = []
    for b in range(8, bars):
        stabs += [N(55, b * 4.0, 0.15, 120), N(58, b * 4.0 + 2.0, 0.12, 110)]
    lines: list[str] = []
    write_header(lines, "Skillet Skirmish - iso battle INT", "Gm funk INT: bed / lead / drums / stabs.", 134)
    track_pulse(lines, "Pad", "pad", bars, "25", 5, bed_pad, layer=0, vib=(4, 6))
    track_pulse(lines, "Lead", "lead", bars, "25", 13, lead, layer=1, arp=(8, 7), vib=(4, 8))
    track_wave(lines, bars, "square", bass, layer=0)
    track_noise(lines, bars, hats_noise(bars, "eighth"), layer=2, vol=6)
    track_kick(lines, bars, kick(bars, "break"), layer=2, drop=-12)
    track_stab(lines, bars, stabs, layer=3, wave="sawtooth")
    save("battle-skillet-skirmish.deck", lines)


def song_boss() -> None:
    # INT boss: chromatic tension E - F - F# - G climbing, then drop to B
    bars = 16
    roots = [40, 41, 42, 43, 40, 41, 42, 43, 35, 35, 40, 42, 43, 42, 41, 40]
    lead_cell = [
        N(64, 0, 0.2, 127), N(65, 0.25, 0.2, 120), N(66, 0.5, 0.2, 125), N(67, 0.75, 0.2, 127),
        N(64, 1.25, 0.25, 120), N(67, 1.5, 0.25, 125), N(70, 1.75, 0.25, 127),
        N(72, 2.25, 0.5, 127), N(70, 2.9, 0.35, 120), N(67, 3.4, 0.5, 125),
    ]
    lead = repeat_section(lead_cell, 1, 16)
    stabs = []
    for b in range(bars):
        if b >= 4:
            stabs.append(N(48 + (b % 4), b * 4.0 + 0.0, 0.12, 127))
        if b >= 8:
            stabs.append(N(60, b * 4.0 + 1.5, 0.1, 120))
        if b >= 12:
            stabs += [N(64, b * 4.0 + 0.5, 0.1, 127), N(64, b * 4.0 + 2.5, 0.1, 127)]
    lines: list[str] = []
    write_header(lines, "Kitchen Showdown - boss INT", "Chromatic climb INT. Alarm stabs at peak.", 152)
    track_pulse(lines, "Pad", "pad", bars, "25", 5, pad_from_roots(roots, voicing="cluster"), layer=0)
    track_pulse(lines, "Lead", "lead", bars, "12_5", 14, lead, layer=1, arp=(14, 12), vib=(5, 10))
    track_wave(lines, bars, "saw", bass_from_roots(roots, "drive"), layer=0)
    track_noise(lines, bars, hats_noise(bars, "16th"), layer=2, vol=8)
    track_kick(lines, bars, kick(bars, "disco"), layer=2, drop=-14)
    track_stab(lines, bars, stabs, layer=3, wave="pulse")
    save("boss-kitchen-showdown.deck", lines)


def song_card() -> None:
    # Shuffle ostinato: cycle of 3 — F#m - A - E, then D - A - E - F#m
    bars = 12
    roots = [42, 45, 40, 42, 45, 40, 38, 45, 40, 42, 40, 42]
    ost = [
        N(66, 0, 0.25, 110), N(69, 0.25, 0.25, 105), N(73, 0.5, 0.25, 115), N(69, 0.75, 0.25, 100),
        N(66, 1.0, 0.25, 110), N(64, 1.25, 0.25, 100), N(66, 1.5, 0.5, 115),
        N(69, 2.25, 0.25, 110), N(73, 2.5, 0.25, 120), N(76, 2.75, 0.25, 125), N(73, 3.25, 0.7, 115),
    ]
    lead = repeat_section(ost, 1, 12)
    lines: list[str] = []
    write_header(lines, "Recipe Duel - card battle", "3-chord shuffle ostinato. Light disco kick.", 130)
    track_pulse(lines, "Pad", "pad", bars, "12_5", 4, pad_from_roots(roots, voicing="third"), vib=(5, 6))
    track_pulse(lines, "Lead", "lead", bars, "50", 12, lead, vib=(5, 6))
    track_wave(lines, bars, "sine", bass_from_roots(roots, "syncop"))
    track_kick(lines, bars, kick(bars, "disco"), drop=-10)
    track_pcm_hat(lines, bars, hats_pcm(bars, "swing"), "triangle", 7)
    save("battle-card-recipe-duel.deck", lines)


def song_rush() -> None:
    # NEW non-INT battle: Mixolydian G sprint
    bars = 12
    roots = [43, 43, 45, 43, 40, 40, 38, 43, 43, 45, 40, 43]
    cell = [
        N(67, 0, 0.2, 120), N(69, 0.25, 0.2, 115), N(71, 0.5, 0.2, 120), N(74, 0.75, 0.2, 125),
        N(71, 1.1, 0.2, 115), N(69, 1.35, 0.2, 110), N(67, 1.6, 0.35, 120),
        N(62, 2.25, 0.25, 110), N(67, 2.6, 0.25, 120), N(71, 2.95, 0.25, 125), N(74, 3.35, 0.55, 127),
    ]
    lead = repeat_section(cell, 1, 12)
    lines: list[str] = []
    write_header(lines, "Rush Hour - quick battle", "Mixolydian sprint. No intensifier layers.", 144)
    track_pulse(lines, "Pad", "pad", bars, "25", 4, pad_from_roots(roots, voicing="root"))
    track_pulse(lines, "Lead", "lead", bars, "50", 13, lead)
    track_wave(lines, bars, "square", bass_from_roots(roots, "drive"))
    track_noise(lines, bars, hats_noise(bars, "eighth"), vol=6)
    track_kick(lines, bars, kick(bars, "four"), drop=-12)
    save("battle-rush-hour.deck", lines)


def song_deploy() -> None:
    # Anticipatory: sus chords Csus - Asus - Fsus - G
    bars = 12
    roots = [36, 33, 41, 43, 36, 33, 41, 43, 36, 38, 43, 36]
    lead = []
    for bi in range(bars):
        t = bi * 4.0
        lead += [N(60 + (bi % 3) * 2, t + 0.0, 0.15, 90), N(67, t + 2.0, 0.15, 100)]
        if bi % 4 == 3:
            lead += spoon_once(t + 2.5, 60)
    lines: list[str] = []
    write_header(lines, "Mise en Place - deploy", "Sus ticks + occasional spoon. Prep tension.", 108)
    track_pulse(lines, "Pad", "pad", bars, "25", 4, pad_from_roots(roots, voicing="fifth"), vib=(2, 8))
    track_pulse(lines, "Lead", "lead", bars, "50", 9, lead)
    track_wave(lines, bars, "sine", bass_from_roots(roots, "half"))
    track_kick(lines, bars, kick(bars, "half"), drop=-8)
    track_pcm_hat(lines, bars, hats_pcm(bars, "offbeat"), "triangle", 5)
    save("deploy-mise-en-place.deck", lines)


def song_holo() -> None:
    # NEW ambient lounge for dialog beds
    bars = 16
    roots = [45, 48, 43, 40, 45, 48, 50, 45, 41, 43, 45, 40, 45, 48, 43, 45]
    lead = []
    for bi in range(0, bars, 2):
        t = bi * 4.0
        lead += [N(69, t + 0.5, 3.0, 60), N(72, t + 4.5, 2.5, 55)]
    lines: list[str] = []
    write_header(lines, "Holo Lounge - dialog bed", "Near-silent pad lounge under dialogue.", 72)
    track_pulse(lines, "Pad", "pad", bars, "12_5", 3, pad_from_roots(roots, voicing="third"), vib=(1, 12))
    track_pulse(lines, "Lead", "lead", bars, "25", 6, lead, vib=(2, 14))
    track_wave(lines, bars, "sine", bass_from_roots(roots, "pedal"))
    track_kick(lines, bars, [], drop=-6)
    track_pcm_hat(lines, bars, hats_pcm(bars, "none"), "sine", 3)
    save("ambient-holo-lounge.deck", lines)


# ── EDM club pack (chip approximations of techno / house / acid / …) ─────────

def song_techno() -> None:
    # Warehouse techno: Am pedal, hypnotic 16th pulse, relentless four-on-floor.
    bars = 16
    roots = [33] * 8 + [36, 36, 33, 33, 31, 31, 33, 33]
    # Berlin-style stab: short gate on offbeats
    pad = []
    for bi, r in enumerate(roots):
        t = bi * 4.0
        p = r + 24
        pad += [N(p, t + 0.5, 0.2, 70), N(p, t + 2.5, 0.2, 65)]
    # Hypnotic lead: 16ths circling A minor pent
    tones = [57, 60, 62, 64, 67, 64, 62, 60]
    lead = []
    for bi in range(bars):
        t = bi * 4.0
        for i, m in enumerate(tones * 2):
            # drop a few hits for air every other bar
            if bi % 2 == 1 and i in (3, 7, 11, 15):
                continue
            lead.append(N(m + (12 if bi >= 8 and i % 8 < 2 else 0), t + i * 0.25, 0.2, 100 + (i % 4) * 5))
    lines: list[str] = []
    write_header(lines, "Warehouse Floor - techno", "Am pedal techno. 16th hypnotic gate. No melody fluff.", 132)
    track_pulse(lines, "Pad", "pad", bars, "12_5", 4, pad)
    track_pulse(lines, "Lead", "lead", bars, "25", 11, lead)
    track_wave(lines, bars, "square", bass_from_roots(roots, "drive"))
    track_noise(lines, bars, hats_noise(bars, "16th"), vol=5)
    track_kick(lines, bars, kick(bars, "four"), drop=-14)
    save("edm-warehouse-techno.deck", lines)


def song_house() -> None:
    # Classic house: Cm7 - F7 - Bbmaj7 - Eb, piano stabs + offbeat hats + disco kick.
    bars = 16
    roots = [36, 41, 46, 39, 36, 41, 46, 39, 36, 41, 34, 39, 36, 41, 46, 36]
    # Chord stabs (pulse pad playing short gated chords as single notes on 3rds)
    stabs = []
    for bi, r in enumerate(roots):
        t = bi * 4.0
        mid = r + 24
        for o in (0.0, 1.5, 3.0):
            stabs.append(N(mid + 3, t + o, 0.35, 95))
    # Funky lead hook
    hook = [
        N(63, 0, 0.5, 115), N(65, 0.5, 0.25, 110), N(67, 0.75, 0.25, 120),
        N(70, 1.25, 0.5, 125), N(67, 1.9, 0.35, 115),
        N(63, 2.5, 0.5, 110), N(58, 3.25, 0.7, 105),
    ]
    lead = repeat_section(hook, 1, 8) + shift([
        N(70, 0, 0.5, 120), N(72, 0.5, 0.5, 125), N(75, 1, 0.5, 127), N(72, 1.5, 0.5, 120),
        N(70, 2.25, 0.5, 115), N(67, 2.9, 0.35, 110), N(63, 3.4, 0.55, 105),
    ], 32) + shift(repeat_section(hook, 1, 7), 36)
    lines: list[str] = []
    write_header(lines, "Gravity House - house", "Cm house changes. Piano-ish stabs + offbeat hats.", 124)
    track_pulse(lines, "Pad", "pad", bars, "50", 6, stabs)
    track_pulse(lines, "Lead", "lead", bars, "50", 12, lead, vib=(3, 6))
    track_wave(lines, bars, "sine", bass_from_roots(roots, "syncop"))
    track_kick(lines, bars, kick(bars, "disco"), drop=-11)
    track_pcm_hat(lines, bars, hats_pcm(bars, "offbeat"), "triangle", 9)
    save("edm-gravity-house.deck", lines)


def song_acid() -> None:
    # Acid: 303-style saw wave bassline (16ths, slides via neighbor tones), dry kick, open hats.
    bars = 16
    # Classic minor acid pattern that mutates every 4 bars
    patterns = [
        [36, 36, 48, 36, 39, 36, 43, 39, 36, 48, 36, 39, 41, 39, 36, 34],
        [36, 39, 48, 39, 36, 43, 48, 43, 36, 39, 41, 43, 48, 43, 39, 36],
        [34, 36, 48, 36, 39, 41, 43, 48, 43, 41, 39, 36, 34, 36, 39, 36],
        [36, 36, 36, 48, 39, 39, 43, 48, 36, 41, 43, 48, 51, 48, 43, 39],
    ]
    acid: list[Event] = []
    for bi in range(bars):
        t = bi * 4.0
        pat = patterns[(bi // 4) % len(patterns)]
        for i, m in enumerate(pat):
            # accent every 4th; shorter notes = more "slide" feel between hits
            vel = 125 if i % 4 == 0 else (100 if i % 2 == 0 else 85)
            acid.append(N(m, t + i * 0.25, 0.22, vel))
    # Sparse screech lead (high pulse)
    screech = []
    for bi in range(bars):
        if bi % 4 == 2:
            t = bi * 4.0
            screech += [N(72, t + 0.5, 0.15, 110), N(75, t + 1.5, 0.15, 115), N(79, t + 3.0, 0.4, 120)]
    lines: list[str] = []
    write_header(lines, "Acid Reactor - acid", "303-ish saw 16ths. Mutating patterns. Dry techno kick.", 130)
    track_pulse(lines, "Pad", "pad", bars, "12_5", 3, screech)
    # Lead = quiet high echo of acid accents
    echo = [N(m + 24, s, 0.12, max(60, v - 40)) for m, s, d, v in acid if int(s * 4) % 8 == 0]
    track_pulse(lines, "Lead", "lead", bars, "25", 8, echo)
    track_wave(lines, bars, "saw", acid)
    track_noise(lines, bars, hats_noise(bars, "offbeat"), vol=6)
    track_kick(lines, bars, kick(bars, "four"), drop=-13)
    save("edm-acid-reactor.deck", lines)


def song_trance() -> None:
    # Uplifting trance: Am - F - C - G build, supersaw-ish arp, rolling bass.
    bars = 16
    roots = [33, 41, 36, 43, 33, 41, 36, 43, 33, 41, 38, 43, 33, 41, 36, 43]
    arp_tones = [0, 3, 7, 12, 15, 12, 7, 3]
    lead = []
    for bi, r in enumerate(roots):
        t = bi * 4.0
        base = r + 24
        for i, off in enumerate(arp_tones * 2):
            lead.append(N(base + off, t + i * 0.25, 0.22, 95 + (i % 4) * 6))
    # Big gated pad hits on bar starts in second half
    pad = []
    for bi, r in enumerate(roots):
        t = bi * 4.0
        p = r + 24
        if bi >= 8:
            pad.append(N(p + 7, t, 1.5, 80))
        else:
            pad.append(N(p + 7, t, 3.5, 55))
    lines: list[str] = []
    write_header(lines, "Trance Orbit - trance", "Am-F-C-G arp rush. Gate opens after bar 8.", 138)
    track_pulse(lines, "Pad", "pad", bars, "50", 5, pad, vib=(4, 10))
    track_pulse(lines, "Lead", "lead", bars, "50", 12, lead, vib=(5, 8))
    track_wave(lines, bars, "saw", bass_from_roots(roots, "drive"))
    track_kick(lines, bars, kick(bars, "four"), drop=-12)
    track_pcm_hat(lines, bars, hats_pcm(bars, "eighth"), "triangle", 7)
    save("edm-trance-orbit.deck", lines)


def song_breaks() -> None:
    # Breakbeat / jungle-lite: Amen-ish kick pattern, ragga-ish stab, fast hats.
    bars = 12
    roots = [36, 36, 39, 36, 34, 34, 36, 39, 36, 41, 39, 36]
    # Broken kick pattern (not four-on-floor)
    kicks: list[Event] = []
    for bi in range(bars):
        t = bi * 4.0
        for o, v in ((0.0, 127), (0.75, 100), (1.5, 120), (2.5, 110), (3.25, 95)):
            kicks.append(N(34, t + o, 0.1, v))
    stab = [
        N(60, 0, 0.2, 120), N(63, 0.25, 0.2, 115), N(67, 0.5, 0.35, 125),
        N(60, 1.5, 0.2, 110), N(58, 2.0, 0.4, 115), N(55, 2.75, 0.5, 105),
        N(60, 3.5, 0.4, 120),
    ]
    lead = repeat_section(stab, 1, 12)
    lines: list[str] = []
    write_header(lines, "Chopped Breaks - breakbeat", "Amen-ish kicks. Ragga stab. Fast hats.", 160)
    track_pulse(lines, "Pad", "pad", bars, "25", 4, pad_from_roots(roots, voicing="cluster"))
    track_pulse(lines, "Lead", "lead", bars, "12_5", 13, lead)
    track_wave(lines, bars, "square", bass_from_roots(roots, "syncop"))
    track_noise(lines, bars, hats_noise(bars, "16th"), vol=7)
    track_kick(lines, bars, kicks, drop=-14)
    save("edm-chopped-breaks.deck", lines)


def song_dub() -> None:
    # Dub techno: very sparse, long decays via vib, delay-ish echoes as repeated quiet notes.
    bars = 16
    roots = [38] * 4 + [36] * 4 + [41] * 4 + [38] * 4
    pad = []
    lead = []
    for bi, r in enumerate(roots):
        t = bi * 4.0
        pad.append(N(r + 24, t, 3.8, 50))
        if bi % 4 == 0:
            lead += [
                N(62, t + 0.5, 0.8, 90),
                N(62, t + 1.5, 0.5, 55),  # "delay"
                N(62, t + 2.25, 0.4, 35),
            ]
        if bi % 4 == 2:
            lead += [N(58, t + 1.0, 1.0, 80), N(58, t + 2.5, 0.6, 45)]
    lines: list[str] = []
    write_header(lines, "Dub Fridge - dub techno", "Sparse Dub techno. Fake delay taps. Half-time kick.", 118)
    track_pulse(lines, "Pad", "pad", bars, "12_5", 4, pad, vib=(1, 16))
    track_pulse(lines, "Lead", "lead", bars, "25", 9, lead, vib=(2, 12))
    track_wave(lines, bars, "sine", bass_from_roots(roots, "pedal"))
    track_kick(lines, bars, kick(bars, "half"), drop=-9)
    track_pcm_hat(lines, bars, hats_pcm(bars, "drip"), "sine", 4)
    save("edm-dub-fridge.deck", lines)


def song_electro() -> None:
    # Electro / freestyle: syncopated bass, cowbell-ish high pulse, crisp hats.
    bars = 12
    roots = [36, 36, 39, 41, 36, 34, 36, 39, 41, 43, 39, 36]
    bass = bass_from_roots(roots, "syncop")
    cowbell = []
    for bi in range(bars):
        t = bi * 4.0
        for o in (0.5, 1.5, 2.25, 3.5):
            cowbell.append(N(84, t + o, 0.08, 100))
    riff = [
        N(60, 0, 0.25, 120), N(63, 0.5, 0.25, 115), N(67, 1.0, 0.25, 125),
        N(60, 1.5, 0.25, 110), N(58, 2.0, 0.5, 115), N(55, 2.75, 0.25, 105),
        N(58, 3.25, 0.25, 110), N(60, 3.5, 0.45, 120),
    ]
    lead = repeat_section(riff, 1, 12)
    lines: list[str] = []
    write_header(lines, "Electro Alley - electro", "Syncop bass + cowbell ticks. Freestyle riff.", 128)
    track_pulse(lines, "Pad", "pad", bars, "50", 5, cowbell)
    track_pulse(lines, "Lead", "lead", bars, "25", 12, lead)
    track_wave(lines, bars, "square", bass)
    track_kick(lines, bars, kick(bars, "break"), drop=-12)
    track_pcm_hat(lines, bars, hats_pcm(bars, "clave"), "triangle", 8)
    save("edm-electro-alley.deck", lines)


def write_sting(name: str, title: str, comment: str, bpm: int, kind: str) -> None:
    lines: list[str] = []
    write_header(lines, title, comment, bpm)
    if kind == "victory":
        lines += [
            "track Lead id lead gen gameBoyDmg",
            "  gen type pulse duty 50 env_mode step vol 14",
        ]
        emit(lines, spoon_once(0, 60) + [N(72, 0.9, 0.3, 120), N(76, 1.25, 0.25, 125), N(79, 1.55, 0.5, 127), N(84, 2.2, 0.7, 120)])
        lines += ["", "track Bass id bass gen gameBoyDmg", "  gen type wave wave_shape saw vol 14"]
        emit(lines, [N(36, 0, 0.5, 120), N(43, 0.5, 0.5, 115), N(48, 1.0, 0.5, 120), N(36, 1.5, 1.0, 125)])
        lines += ["", "track Crash id crash gen gbaDirectSound",
                  "  gen waveform triangle vol 12 bitcrush true", "  adsr a 0 d 0.25 s 0 r 0.2"]
        emit(lines, [N(48, 0.7, 0.4, 127)])
    elif kind == "defeat":
        lines += ["track Lead id lead gen gameBoyDmg", "  gen type pulse duty 25 env_mode step vol 12"]
        emit(lines, burn_once(0, 67) + [N(62, 0.9, 0.5, 90), N(55, 1.5, 0.8, 80), N(51, 2.4, 1.0, 70)])
        lines += ["", "track Noise id n gen gameBoyDmg",
                  "  gen type noise noise_mode short vol 8 env_mode step env_step 3"]
        emit(lines, [N(60, 0.3, 0.4, 100), N(60, 1.2, 0.6, 80)])
        lines += ["", "track Drop id drop gen gbaDirectSound",
                  "  gen waveform sawtooth vol 10 bitcrush true pitch_drop -8 pitch_dec 0.25",
                  "  adsr a 0 d 0.3 s 0 r 0.15"]
        emit(lines, [N(40, 0.5, 0.5, 110)])
    elif kind == "level":
        lines += ["track Lead id lead gen gameBoyDmg", "  gen type pulse duty 50 env_mode step vol 13"]
        emit(lines, [N(72, 0, 0.1, 120), N(76, 0.12, 0.1, 115), N(79, 0.24, 0.15, 125),
                     N(84, 0.42, 0.2, 127), N(88, 0.7, 0.25, 120), N(84, 1.05, 0.4, 115)])
        lines += ["", "track Shimmer id shim gen gbaDirectSound",
                  "  gen waveform sine vol 10 bitcrush true", "  adsr a 0 d 0.25 s 0 r 0.15"]
        emit(lines, [N(96, 0.3, 0.5, 90)])
    elif kind == "sidequest":
        lines += ["track Lead id lead gen gameBoyDmg", "  gen type pulse duty 50 env_mode constant vol 12"]
        emit(lines, [N(69, 0, 0.2, 110), N(71, 0.25, 0.2, 105), N(74, 0.5, 0.3, 120),
                     N(78, 0.95, 0.35, 125), N(74, 1.45, 0.25, 110), N(71, 1.85, 0.5, 105)])
        lines += ["", "track Bell id bell gen gbaDirectSound",
                  "  gen waveform triangle vol 9 bitcrush true", "  adsr a 0 d 0.15 s 0 r 0.1"]
        emit(lines, [N(90, 0.6, 0.3, 100)])
    else:  # law
        lines += ["track Lead id lead gen gameBoyDmg", "  gen type pulse duty 12_5 env_mode step vol 14"]
        emit(lines, [N(88, 0, 0.12, 127), N(88, 0.2, 0.12, 120), N(81, 0.45, 0.3, 110)] + burn_once(0.9, 69))
        lines += ["", "track Noise id n gen gameBoyDmg",
                  "  gen type noise noise_mode short vol 7 env_mode step env_step 1"]
        emit(lines, [N(60, 0.1, 0.08, 90), N(60, 0.3, 0.08, 90), N(60, 1.0, 0.25, 110)])
    save(name, lines)


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    song_title()
    song_hub()
    song_menu()
    song_shop()
    song_hyperspace()
    song_agri()
    song_night()
    song_liner()
    song_asteroid()
    song_icebox()
    song_spice()
    song_sugar()
    song_galley()
    song_compactor()
    song_skirmish()
    song_boss()
    song_card()
    song_rush()
    song_deploy()
    song_holo()
    song_techno()
    song_house()
    song_acid()
    song_trance()
    song_breaks()
    song_dub()
    song_electro()
    write_sting("sting-plated-victory.deck", "Plated! - victory", "Spoon fanfare climb.", 150, "victory")
    write_sting("sting-burned-defeat.deck", "Burned! - defeat", "Burn motif + drop.", 90, "defeat")
    write_sting("sting-level-up.deck", "New Recipe - level up", "Pentatonic sparkle.", 140, "level")
    write_sting("sting-sidequest.deck", "Side Order - side quest", "Curious sixth leap.", 120, "sidequest")
    write_sting("sting-law-whistle.deck", "Health Inspector - law", "Double whistle + burn.", 130, "law")
    print(f"done -> {OUT}")


if __name__ == "__main__":
    main()
