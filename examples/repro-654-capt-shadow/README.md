# REPRO 654 CAPT SHADOW

> *A body-local must shadow its never-assigned `_capt` alias.*

<img src="preview.png" alt="preview" width="480">

Isolates an E0369 from a topdown RPG port: an outer never-assigned binding snapped to `_capt`, and the body-local of the same name must shadow the alias so comparisons use the local, not the capture.
