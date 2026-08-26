# REPRO 654 SIBLING STACK

> *One hot FunDecl naming ~40 sibling fns keeps VmRefs, not Value extracts.*

<img src="preview.png" alt="preview" width="480">

Isolates a large-SRPG pattern: a hot function that names ~40 sibling functions must keep `VmRef` clones at entry rather than extracting Values.
