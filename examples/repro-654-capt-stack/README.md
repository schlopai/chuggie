# REPRO 654 CAPT STACK

> *~40 never-assigned Value captures + a nested arrow should not clone at entry.*

<img src="preview.png" alt="preview" width="480">

Asserts a FunDecl with ~40 never-assigned captures and a nested `move` arrow does not emit mass `let mut X = X_capt.clone()` Value locals at function entry.
