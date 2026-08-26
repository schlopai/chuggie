# Building, testing and self-playing a GBA example

Written for an agent working in this repo. It describes the loop actually used to build the
isoboard examples — the commands, and more importantly the
traps that cost real time. The commands below use `<example>` for whichever example you are building.

The shape of the loop is: **change → build → drive the ROM headlessly → read the result → strip your
instrumentation → prove you did not regress.** There is no interactive step. Every claim in this
repo's docs was produced by a command another agent can re-run.

---

## 1. Build

From the example directory:

```bash
cd examples/<example>
unset CARGO_TARGET_DIR
TISH_FAST_NATIVE_BUILD=1 npm run build   # day-to-day iteration
# unset CARGO_TARGET_DIR && npm run build  # default: thin LTO, still release-quality
```

**GBA cargo profiles** (written into `.tish/gba/<name>/Cargo.toml` by `tish`):

| Env | Use when |
|-----|----------|
| `TISH_FAST_NATIVE_BUILD=1` | Iterating. No LTO, opt-level 1 — often several× faster. ⚠️ exits 0 on a failed GBA compile and leaves the OLD `.gba` in place. |
| _(default)_ | **Fat LTO, 1 CGU, opt-level 3.** Smallest stack frames, which is the binding constraint on this target. |
| `TISH_GBA_THIN_LTO=1` | Faster links on a ROM with stack headroom to spare — **check first**, see below. |
| `TISH_GBA_FAT_LTO=1` | Legacy; a no-op now that fat is the default. Kept so old scripts keep working. |
| `TISH_GBA_DEBUG=1` | Need release debuginfo for mgba backtraces. |

### ⚠️ Thin LTO costs STACK, and the overflow does not crash

Thin LTO was briefly the tish default (#581). It was reverted because **less inlining means bigger
stack frames**, and the GBA gives the entire stack 32 KB of IWRAM. Measured on the large SRPG example, same
source, cold `.tish`:

| profile | `run()` | shell factory | combined | vs 32,512 B usable |
|---|---:|---:|---:|---|
| fat | 23,604 | 7,452 | **31,056** | fits, 1,456 B spare |
| thin | 25,436 | 9,068 | **34,504** | **over by 1,992 B** |

The dangerous part: **it still ran.** The deepest SP under thin is `0x02FFF838`, and the GBA mirrors
EWRAM every 256 KB, so it aliases `0x0203F838` — the top of the heap. The stack quietly writes into
allocated memory instead of trapping, so the damage surfaces later and somewhere unrelated. That example
reached `acts: Shara 1` with 0 faults on thin purely by luck about what occupied that region.

Build time is example-dependent, so measure rather than assume:

| example | thin | fat |
|---|---:|---:|
| the HUD SRPG demo (small) | **70 s** | 86 s — thin also 37 KB smaller, frames byte-identical |
| the large SRPG example | 148 s | **137 s** — thin was slower |

**Before opting in, check the ROM's headroom.** Read the frame constant out of `run()`'s prologue and
add the frame of any factory it calls — both are live at once, since the factory is called *from*
`run()`:

```bash
nm -n .tish/gba/<name>/target/thumbv4t-none-eabi/release/<name> | grep '3run$'
# large frames are `ldr rN, [pc, …]` + `add sp, rN` with a negative literal, not `sub sp, #imm`
```

Measure profiles and codegen line counts with `examples/bench-build` (`npm run bench`) — **not** by
rebuilding the largest game. Engine games no longer merge-import all of `packages/ui.tish` just for
`loadScene` (that lives in `packages/scene_hooks.tish`).

> **Trap — `CARGO_TARGET_DIR`.** If it is set in the environment, cargo writes the ELF somewhere else
> while `tish build` packages the stale one from the local `target/`, and you get a **byte-identical
> ROM after a real source change**. This wastes an entire debugging session, because every symptom
> points at your code. Always `unset CARGO_TARGET_DIR` in the same command as the build. If a change
> you are certain about appears to do nothing, check this first.

Success looks like a single line: `Built: <example>.gba`. Filter for it and for errors:

```bash
unset CARGO_TARGET_DIR && npm run build 2>&1 | rg -i "Built:|error\[" -A5 | head -10
```

## 2. Look at it — headless screenshots

`scripts/screenshot.sh` renders a ROM through libmgba with no window and no macOS permissions.

```bash
scripts/screenshot.sh <rom.gba | src/main.tish> [out.png] [frames] [keys]
```

`frames` is how many frames to run before capturing (~60 = 1 second). `keys` is either keys held for
the whole run (`"a,start"`) or a **frame schedule** (`"90:a,120:"`). Key names: `a b select start
right left up down r l`.

Read the PNG back with the image reader — the model can see it. This is the primary form of evidence
in this repo: a screenshot showing the thing working is worth more than a passing assertion.

When the thing to show is *motion* — a walk cycle, a scene transition, a particle burst, an AI
actually reacting — a still cannot carry it. `scripts/gif.sh` takes the same four arguments and
records the run as a looping GIF out of a single emulator boot:

```bash
scripts/gif.sh <rom.gba | src/main.tish> [out.gif] [frames] [keys]
```

Recording never opens on a blank frame — a GBA spends its first frames on flat black or white, and
a clip that starts there looks broken — so `GIF_FROM` is a floor, not an exact start, and the first
frame you get is the first one with a picture on it. (`GBA_SHOT_SEQ_BLANK=1` turns that guard off.)
Only the opening is guarded: a deliberate fade to black mid-clip is recorded like anything else.

Tune it with `GIF_FROM` (first frame recorded, default 60 — skips the boot frames), `GIF_EVERY`
(record one frame in N, default 3 ≈ 20fps), `GIF_SCALE` (default 2) and `GIF_MAX_FRAMES`
(default 300). Every example exposes it as `npm run gif`, inheriting that example's own `shot`
frame count and key schedule.

> **Trap — a schedule entry HOLDS until the next entry.** `"300:right,320:right"` presses right
> *once*: the key was already down at 320, so no new press is registered. Every press needs an
> explicit release:
>
> ```
> "300:right,310:,320:right,330:,360:a,370:"
> ```
>
> If a navigation script lands one menu short of where you expected, this is why.

> **Trap — do not build schedules in zsh.** `$f:up` is a zsh parameter modifier (`:u` = uppercase),
> so a loop that assembles `"$f:up"` silently produces `492p` instead of `492:up`. Generate schedules
> with a `python3` heredoc instead.

> **Trap — text can lose glyphs mid-board.** HUD text is drawn as sprites. Dense rows of terrain
> (which are also sprites) exhaust the GBA's per-scanline object budget and later glyphs are simply
> dropped, so a line renders as `Spoils: Pot`. It is **not** a layering or truncation bug. The fix is
> position, not priority: the top row and the rows just above the action menu are clear; the middle
> of the board is not.

## 3. Read what it is thinking — logs

For anything a screenshot cannot show (which unit chose what, whether a branch ran), instrument
temporarily and capture with `GBA_SHOT_LOG=1`:

```tish
import { log } from 'cargo:tish_agb'
log("ITEM " + ability.name + " left " + STOCK[i].count)
```

```bash
GBA_SHOT_LOG=1 scripts/screenshot.sh examples/<example>/<example>.gba /tmp/x.png 20000 "" 2>&1 \
  | rg "ITEM " | head -20
```

Output is prefixed with the frame it happened on, which is what makes it usable for timing as well as
for logic. The stream also carries a lot of emulator DMA noise — always filter.

Two more environment switches on `tools/gba-shot`:

- `GBA_SHOT_TRACE=1` reports every frame on which the **picture changed**. This is the objective
  measure of a load or menu budget, with no ROM instrumentation: it says which frame the player first
  saw the new screen.
- `GBA_SHOT_AUDIO=<out.wav>` captures emulated sound to a WAV plus an `audio:` summary line (samples,
  peak, RMS), so "does it actually play, and what note" is assertable.

### Two consoles on a cable — `scripts/link.sh`

`tools/gba-shot` runs ONE core, so a link-cable game always sees an empty cable: it takes its
offline path, and the whole transport — register writes, master/child split, the transfer
handshake — is executed by no test at all.

`scripts/link.sh <rom> [rom2] [frames] [keys0] [keys1]` runs two cores in one process with their
serial ports wired through mGBA's own `GBASIOLockstep`, and prefixes each console's output with its
id. That is a real test: it found three protocol bugs that reading the code did not (a master
waiting on a readiness bit only its own transfer could establish; a handshake that stopped sending
the seed before the peer had it; and an id field that reads 0 on BOTH units until the port
enumerates, so `other = 1 - id` had the child reading its own slot).

Two gotchas if you write another one of these. `mLockstepInit` leaves `signal`/`wait`/`addCycles`/
`useCycles` NULL — only `lock`/`unlock` are null-checked — so the first transfer segfaults until
you supply them. And the cores must be interleaved in SMALL SLICES via `core->step` (~128
instructions), not whole `runFrame`s: a whole frame lets the master start a transfer and finish its
frame before the child is stepped, so the transfer never completes and the link looks dead.

`examples/link-demo` is the human-facing half — state, role, seed, round-trip and a live button
mirror, for two mGBA windows (**File → New multiplayer window**) or two cartridges.

> **Trap — `log` must be imported**, and Tish resolves names in source order at statement level. A
> helper used by a statement before its definition is not in scope; forward references *inside*
> function bodies are fine. When a probe fails with "not found in scope", move the definition up
> rather than assuming the language hoists.

## 4. Self-play — the regression harness

An isoboard example can carry a `SELF_PLAY` flag near the top of `src/main.tish`;
the same pattern applies to any battle example:

```tish
let SELF_PLAY = 0   // 0 = team 0 is the player; 1 = attract mode, both teams AI
```

With `SELF_PLAY = 1` the battle plays itself to a result with no input, which is the repo's standard
regression: **build it, run it to several depths, and check it still reaches a clean terminal frame.**

```bash
cp src/main.tish /tmp/main.ship.tish                      # keep the shipping version
python3 -c "p='src/main.tish';s=open(p).read();s=s.replace('let SELF_PLAY = 0','let SELF_PLAY = 1',1);open(p,'w').write(s)"
unset CARGO_TARGET_DIR && npm run build
cd ../..
for f in 4000 20000; do scripts/screenshot.sh examples/<example>/<example>.gba /tmp/reg_$f.png $f ""; done
```

Then **restore and rebuild the shipped ROM** — the artifact on disk must be the playable one:

```bash
cp /tmp/main.ship.tish examples/<example>/src/main.tish
cd examples/<example> && unset CARGO_TARGET_DIR && npm run build
rg -n "SELF_PLAY = 0$" src/main.tish     # confirm
```

Attract mode also bypasses the title screen and takes Auto Deploy automatically. That is deliberate:
a debug path that needs a recompile does not get used.

> **Trap — compare against a baseline before blaming yourself.** A self-play run ending in `Defeat`
> at frame 2500 looks like your bug. Stash your work, build `HEAD`, run the identical command, and
> compare. In this repo that exact result turned out to be pre-existing every time it was checked:
>
> ```bash
> git stash push -- examples/<example> crates/tish-gba-game-engine
> # …build and run the baseline…
> git checkout -- examples/<example>/src/main.tish && git stash pop
> ```
>
> Verify the restore with a `diff` against a copy you made first. Do not skip this step; a stash
> round-trip that silently drops work is worse than the bug you were chasing.

## 5. Forcing a scenario the ordinary battle never reaches

The demo battle is short and one-sided, so features that only matter in rare states — healing items,
revival, a currency that accrues slowly — will **never fire on their own**. Waiting for them is not
verification, it is hoping.

Instead, plant a one-shot probe at the first turn that creates the state directly, run attract mode,
and read the log:

```tish
let probed = 0
function itemProbe() {
  isob_damage(0, isob_unit_maxhp(0) - 4)   // Fighter nearly dead -> wants a Potion
  unitMp[2] = 0                          // Cleric out of MP    -> wants an Ether
  inflict(1, ST_POISON, 6)               // Mage poisoned       -> wants an Antidote
}
function beginTurn() {
  if (probed === 0) { probed = 1; itemProbe() }
  ...
```

`isob_unit_set_pos` can also place units where the interaction you are testing becomes possible (for
example gathering a squad around one enemy so a Combo has somebody to pull in). This turned a feature
that fired **zero** times in a normal battle into one that fired six times in the right order.

**Strip every probe afterwards** and grep to prove it:

```bash
rg -n "log\(|Probe|probed" src/*.tish
```

## 6. Playing it for real

```bash
npm run play                          # repo root: last example, windowed
scripts/rom.sh play <example>          # by name; runs the ROM ON DISK, does not build
scripts/rom.sh build <example>
scripts/rom.sh shot  <example>
```

`play` deliberately does not build, so it starts instantly; it tells you if the ROM is missing or
older than its sources. Editor tasks in `.vscode/tasks.json` wrap the same script and are regenerated
with `npm run vscode` after adding an example.

---

## The loop, as steps

1. **Read before writing.** Confirm the helper you are about to call exists (`rg -n "pub fn isob_"`),
   and that a field means what you think.
2. **Check the facts if the feature is a parity claim.** This repo matches a specific game; several
   features here were built, then found to be from the wrong title, and had to be dropped. Search
   first, cite in the doc, then implement.
3. **Make the change.** Prefer one build carrying several related edits over several builds.
4. **Build**, with `CARGO_TARGET_DIR` unset.
5. **Screenshot the player-facing result.** If it needs input, write a key schedule with explicit
   releases. Read the PNG back and actually look at it.
6. **Log what a screenshot cannot show**, run headless, filter, strip.
7. **Force the rare state** if the ordinary run does not reach it.
8. **Self-play regression** at two or three depths, and compare with a stashed baseline before
   attributing anything to your change.
9. **Restore the shipping build** (`SELF_PLAY = 0`, probes gone) and rebuild the ROM on disk.
10. **Write down what you learned**, including what did not work — the plan doc records dropped
    features and the reasoning, which is what stops the next agent rebuilding them.

## What counts as evidence here

- A screenshot of the thing working, or a log line showing the branch ran. Not "it should now…".
- For a behaviour that is a distribution rather than an event (hit rates, turn order, tempo), a probe
  that runs it hundreds of times and prints the ratio, then gets deleted.
- For "did I break anything", a self-play run **and** the baseline it is compared against.
- A negative result is a result. Recording that a feature never fires in the shipped battle, and why,
  is more useful to the next agent than a claim that it works.

## A passing smoke test that asserts nothing

The most expensive bugs here were the ones a green `verify.sh` was actively hiding. A run scripted as
"walk into the cave, walk back out" that only greps the log for panics passes just as happily when the
player stands in the field for 600 frames and the door never opens — the ROM did not crash, so the
step prints OK. The topdown RPG port's hub-cave and dungeon steps had been doing exactly that: their key schedules
stopped on the wrong row and entered nothing, through many green runs.

Two things fix this, and both are cheap:

- **Assert the outcome, not the absence of a crash.** `enterScene` logs `SCENE <id>`, so the step can
  require that the destination was reached, and that the last scene is the one you walked back to.
  Any state change worth testing is worth one `log()` line to make it assertable.
- **Watch a schedule before trusting it.** Screenshot the frames around each key press. Walk timings
  are roughly 32 frames per tile, and a schedule that is one tile off looks identical in a log.

Dialogue is the usual reason a schedule silently does nothing: `say()` freezes the player until it is
dismissed, so a `down` issued while an NPC is still talking is simply discarded. If a scripted walk
appears not to move, check for an open dialogue box before suspecting the movement code.

Walk timings vary by example. In the topdown RPG port the hero covers a tile in roughly 13 frames, so a schedule
that stops two tiles short of a doorway looks, in a log, exactly like one that arrived.

## When generated assets and the data behind them disagree

The topdown RPG port's dungeons were built from decoded NES tables by a decode script, and its door
codes were read backwards: the map carved a passage for code 1 and nothing for code 0, when 0 is the
ROM's *open* door and 1 its solid wall. Every one of the nine dungeons was therefore generated as
sealed rooms — under that reading only 12 of 171 rooms are reachable from their entrances, and Level
1's entrance was a closed box with a corridor to nowhere.

Nothing caught it, because everything downstream was consistent with the wrong answer. The maps
looked like dungeons, the room *layouts* really were the NES ones, and an audit that compared room
counts and positions passed. The runtime had no opinion, since `uwDoor()` was a stub returning
"open". Only when the runtime started enforcing door types did the two disagree.

Two habits would have caught it years earlier, and both are worth applying to any generated asset:

- **Ask a question the data must answer, not one the format answers.** "Are the rooms in the right
  places?" is satisfied by nonsense. "Can you walk from the entrance to the boss?" is not — a
  ten-line flood fill over the decoded door graph gave `159 unreachable` for one reading and `0` for
  the other, and that single number settled a question three sessions of reading code had not.
- **Distrust the labels in the decoder itself.** This script carried a `DOOR_NAMES` tuple naming 0
  "wall", a `PASSAGE_DOOR_CODES` set treating 0 as passable, and an `apply_door` carving on 1. Three
  statements of the same fact, two of them wrong. When a constant and the code that uses it
  disagree, believe neither until something external decides.
