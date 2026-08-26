# pong-link

> *Two-player Pong over the link cable: lockstep, where the only thing crossing the wire is a button mask.*

<img src="preview.gif" alt="preview" width="480">

Two-player Pong over the link cable, in lockstep. The only thing that ever crosses the wire is a
button mask.

**Controls** — Up/Down move your paddle · **hold A to sprint** · START begins the match and starts a
rematch. While waiting for a peer, START starts a one-player game against the CPU.

Once two consoles link they show **READY** and wait: the match begins when **player one** — the
master, which is the left paddle by definition — presses START. That press travels through the normal
input exchange, so both consoles begin on the same simulated frame. Starting on a local keypress
would begin the match on one console only.

```bash
npm run build
npm run start     # one console
npm run link      # two consoles, wired together headlessly
npm run verify
```

To play it properly: `mgba-qt pong-link.gba`, then **File → New multiplayer window** and load the ROM
again there. On hardware it is the same cartridge in two consoles and a link cable; boot order and
timing do not matter.

## What this demonstrates that `link-demo` does not

`examples/link-demo` proves the **transport**: two units enumerate, agree a seed, and mirror each
other's buttons. That is necessary and not sufficient — a button mirror does not care *which frame* a
word belongs to, so it cannot catch the class of bug that actually breaks a networked game.

This is a real simulation running on two consoles at once. Neither side ever sends a ball position.
Both exchange buttons and then run the identical simulation on the identical pair of inputs, which
works exactly as long as the simulation is deterministic: the ball is integer fixed point, the serve
angle comes from `packages/rng` seeded with the **link's** agreed seed rather than anything either
console chose locally, and nothing reads a clock the two machines could disagree about.

## One player is a waiting room, not a mode

⚠️ The port is pumped on **every frame that is not already in lockstep**, whether a CPU game is
running or not, and reaching `PLAYING` takes over from whatever was on screen.

The first version got this wrong in a way worth recording. START was both the way into the
one-player game and the thing the screen invites you to press, and taking it set an `online = 0` flag
that nothing ever re-examined. Two mGBA windows need a moment to handshake, so the natural first
keypress produced a console that could never link — which looks exactly like the link being broken,
and was reported as such. A one-player game must never be a decision to stop listening.

## Two speeds, because the simulation is slow

The paddle walks at 7 px per simulated frame and sprints at 14 with A held. At roughly 10 Hz a single
speed is a bad trade — you either crawl across the court or overshoot every ball — and the second
speed lets you ease onto a return with the d-pad and still cover a long diagonal.

⚠️ The sprint is a **fourth bit in the exchanged word**, which moved the round field from bit 3 to
bit 4. Both consoles must agree on that layout exactly; get it wrong and the round number reads off
by a factor of two, so nothing ever pairs again.

⚠️ **START is read HELD, not as an edge.** Only about one frame in six is actually staged onto the
wire, so a single-frame `key_pressed` edge is usually never transmitted — the match simply would not
start. Repeats are harmless, because the phase and the game-over checks both move on first sight.

## The bug this example was built to find

It desynced, and it looked fine. Two screens side by side, both showing a completely plausible game
of Pong, with different scores. **No screenshot of one console can detect that** — which is why the
ROM prints its simulation state every twenty simulated frames and `verify.sh` compares the two.

Three separate faults, in order of discovery:

**A handshake word leaked into the game.** The two units do not reach `PLAYING` on the same round:
the child flips when it receives the seed, the master only when its echo returns one round later. So
the master is still staging a `TAG_SEED` word on a round where the child is already playing, and
`packages/link.tish` only inspected the tag while syncing — once playing it returned any payload. The
child took the seed value, `7919`, as the master's buttons. The master saw no such event. One
asymmetric frame is all a lockstep simulation needs. Fixed in the package: handshake residue is never
game input.

**The pairing was assumed rather than checked.** `linkExchange` alternates — one call stages your
word, the next *ignores its argument* and returns the peer's. Pairing the answer with the buttons
being read when it arrives combines your frame N with the peer's frame N−1, and since the two stage
on opposite frames they build different pairs from the same rounds. Pairing with "the word the link
last staged" is closer and still not a guarantee, because a stall also happens when the port is busy.

**So the word carries a round number** and the pairing is verified. The master owns the clock: it
stamps a round and simulates it only when the child's answer *for that round* comes back; the child
simulates the moment it hears a round and echoes the stamp with the buttons it used. Both simulate
round R with (masterButtons_R, childButtons_R). Anything else arriving is a stale echo and is
ignored, which leaves the staged word in place so the peer hears the round again — idempotent on both
sides, so it recovers instead of deadlocking.

⚠️ **The symmetric version does not work, and it is very tempting.** "Both step when the peer's round
equals mine" is one step per transfer and looks obviously correct. The two consoles stage their words
on their own frame cadence, so a round-R word from one does not travel in the same transfer as
round-R from the other; it ran for twenty or forty rounds, drifted by one, and then neither side ever
matched again. A deadlock that looks exactly like the game freezing.

## The tick is the unit of time, not the simulated frame

⚠️ **Solo runs at 60 Hz. Linked runs at about 10 Hz. They feel the same**, and that is the point.

A linked simulated frame lands about every six display frames, because a round of the protocol costs
two transfers. The first version sized every speed for that — and then threw the *same throttle over
the one-player game*, so a mode with no cable and nothing to wait for also ran at 10 Hz. It moved the
ball eight pixels at a time and answered the d-pad one frame in six. It played badly, and correctly
so.

Now a step runs `G.ticks` ticks and a tick is worth one display frame of motion: one tick per step
solo, six per step linked. Same speed across the court either way, the same integer arithmetic, and
collisions still resolved a tick at a time so a fast ball cannot pass through a six-pixel paddle.
`ticks` is a **constant per mode, never a measured elapsed time** — both consoles must run the
identical number, or they are no longer playing the same game.

Input latency is what remains, and it is inherent: solo answers in one frame, linked in about six
plus the round trip. That is the honest cost of lockstep over this transport; a faster scheme needs
an input delay and a buffer, which is a different design.

## The picture is drawn between simulated frames

A round costs two transfers and the link exchanges a word every other display frame, so a simulated
frame lands roughly every six displayed ones. That is the honest cost of lockstep over this
transport; a faster scheme needs an input delay and a buffer, which is a different design.

⚠️ **Drawing that state directly is what makes a game look chunky**, and it is a presentation problem
rather than a physics one. The ball advances in four collision **ticks** per simulated frame — needed
anyway, because eight pixels at a time goes clean through a six-pixel paddle — and each tick records
its position. The draw path then walks that polyline, so the ball moves under a pixel per display
frame and a bounce *inside* a simulated frame is drawn as a bounce rather than as a straight line
through the wall. Paddles interpolate linearly, which for something that moves once per frame is
exact.

The interpolator measures the frame rate rather than being told it: the gap between simulated frames
depends on the peer, and is different again against the CPU. It clamps at 1, so a late simulated
frame settles the picture on the true state instead of extrapolating into a wall. None of it is
visible to the simulation — both consoles still step identically, they just draw more often than they
think.

## What verify.sh asserts

| Check | What it would catch |
|---|---|
| Builds against the agb **fork** | a stray crates.io `agb`, which cargo only ever warned about — and the warning does not mean what it looks like (see `scripts/check_agb_fork.sh`) |
| Alone: waits for a peer, START starts a CPU game, and it plays | the transport giving up on a missing peer, which on hardware is the normal case |
| Two consoles reach PLAYING, one master one child | two masters means nobody drives transfers and the link is fiction |
| **Both consoles agree at every compared simulation frame** | the desync above — the one thing a single console cannot see |
| Both report the same final score | drift that only shows up at the end |
| An early START still links | the one-player game latching the console offline — the natural first keypress made the cartridge un-linkable |
| Linked but nobody presses START: ball parked, score nil | the match starting on its own — and note both consoles would still *agree*, so the determinism check cannot catch this one |
| The ball moves a little every display frame, never more than 3px | drawing the 10 Hz state directly, which is what "chunky" means |
| A + down travels further than down alone | the sprint bit failing to survive the trip through the word |
| A rematch starts clean and both consoles agree across both matches | a restart running on one console only, which leaves two normal-looking but different matches |
| The winner has exactly the winning score | scoring is tested per tick, and the ticks after match point kept scoring: a first-to-seven finishing 8–2 |
