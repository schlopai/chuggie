# REPRO 654 HEAP NATIVES

> *Many FunDecls naming `log` must not multiply VmRef allocations.*

<img src="preview.png" alt="preview" width="480">

Asserts the `VmRef::new(log…)` count stays tiny across many FunDecls that each name `log`, so the heap does not OOM from duplicated native refs.
