# REPRO HUB CAVE HEAP

> *A bug reproduction or regression test case.*

![preview](preview.png)

Intended isolated repro for hub→cave `AllocError` when a fat boxed index survives `scene_stream`.

The fix in the original game: clear `hubPadAtTile` in `enterScene` when leaving the debug hub (same pattern as
dropping OW spawn indexes before a cave stream).

This harness is incomplete (title/input). Prefer verifying on the topdown RPG port (moved to its own repo): Debug Hub → cave pad → A.
