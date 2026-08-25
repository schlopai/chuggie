# REPRO 654 VEC CELL ARROW

> *A nested arrow over a cell-captured native array must not double-wrap the VmRef.*

Isolates an E0599: a native `i32[]` cell-captured by a FunDecl and indexed from a nested arrow was wrapped `VmRef<VmRef<Vec>>`, breaking `.get`.
