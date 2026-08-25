# Shared pieces for example verify.sh scripts. Source it:
#
#   . "$(dirname "$0")/../../scripts/verify_common.sh"
#
# CRASH_RE — the FULL crash regex. Only one example's verifier carried all of these; the other
# verifiers grepped panic strings alone, which is the exact hole that let a real OOM ship
# unnoticed (the 2026-07 review's D-1). A log is only as honest as the regex run over it:
#   - `Double panic|panicked at`            classic panics that reached the log channel
#   - `memory allocation`                   alloc-error strings that DO get logged
#   - `RefCell already borrowed`            reentrancy bugs
#   - `Illegal opcode|Jumped to invalid address`  the mGBA lines a corrupted jump produces
# NOTE agb's allocation-failure handler HALTS without logging anything — a grep cannot see
# it. That is soak_rom's check 2 (`SWI: 02`); use it for any run-level gate.
CRASH_RE='Double panic|memory allocation|panicked at|RefCell already borrowed|Illegal opcode|Jumped to invalid address'

# crash_grep <log> — exit 0 when the log is CLEAN (so: `crash_grep "$log"; check $? "no crash"`).
crash_grep() {
  ! grep -qE "$CRASH_RE" "$1"
}

# soak_rom <rom.gba> [frames] [key-schedule] — the three-check soak (crash strings, SWI 02
# halts, frame progress >= 90%). Prints its own ok/FAIL lines; exit code is the verdict.
soak_rom() {
  "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/gba_soak.sh" "$@"
}

# assert_agb_fork [example-dir] — check this example resolved `agb` to OUR FORK, not crates.io.
#
# Every example is meant to build against ../agb via the `[patch.crates-io]` block in the repo-root
# .cargo/config.toml, and until now nothing enforced it. What there WAS is a warning on every build —
# "patch `agb ...` was not used in the crate graph" — which reads like the fork is being ignored and
# is not that: `-Z build-std` makes a second resolve for core/alloc, and the patch is unused in that
# one. The real graph does use the fork. A warning that always fires and does not mean what it says
# is the worst kind of guarantee, so ask cargo instead. See scripts/check_agb_fork.sh.
assert_agb_fork() {
  "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/check_agb_fork.sh" "${1:-$PWD}"
}

# assert_typed_scalars [paths...] — fail if any module-level scalar is left UNTYPED.
#
# A `const N = 5`, or a `let N = 5` with no annotation, compiles to a thread-local `Cell<f64>`, so
# every use of it is a SOFT-FLOAT operation on a chip with no FPU — `A[b + A_X]` becomes an i32->f64
# conversion, an f64 add and an f64->usize conversion. Annotating `: i32` makes the same expression
# native integer arithmetic and was measured at 20-27% of a frame across two games.
#
# This is a REGRESSION GATE, not a suggestion: the sweep that fixed 1,500 of them repo-wide is only
# worth anything if the next package does not reintroduce them. See docs/perf-rules.md §1.
assert_typed_scalars() {
  local root
  root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
  local out
  out="$(python3 "$root/scripts/const_to_let.py" --check "${@:-$PWD/src}" 2>&1 | tail -1)"
  case "$out" in
    *"would convert 0 declaration"*) return 0 ;;
    *) echo "$out"; echo "  -> run: python3 scripts/const_to_let.py ${*:-src}"; return 1 ;;
  esac
}
