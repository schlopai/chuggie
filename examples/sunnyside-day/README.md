# Sunnyside day — the clock, the night, and the bed

Sunnyside de-risk 5, part of the `sunnyside` example family (see
`examples/sunnyside/SPEC.md`).  A quarter-minute-per-frame day clock (~96s
per day) with the time-of-day tint that the main game reuses — and a design
decision worth recording: the tint is the GBA's hardware brightness blend
(`fade(level)` → BLDY), not a palette rewrite.  agb's palette entry order is
nondeterministic between builds so naming entries is forbidden, and a BG-bank
rewrite would leave sprites glowing at midnight; BLDY dims backgrounds and
sprites uniformly, costs nothing by day, and needs no new natives.

- 06:00-17:00 full daylight, 17:00-20:00 dusk ramp, then night
- at 02:00 the farmer passes out; A near the barn door sleeps voluntarily
  (fade out, next day, fade in at 06:00)
- HUD shows `D<day> HH:MM`

`./verify.sh` runs a full unattended day and asserts the dusk/night fade
levels, the pass-out → wake sequence, and — with two screenshots — that the
night frame really is at most 75% of the day frame's mean brightness.
