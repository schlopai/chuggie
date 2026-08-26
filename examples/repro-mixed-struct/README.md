# REPRO MIXED STRUCT

> *A struct param qualifies on ONE numeric field, not all of them.*

<img src="preview.png" alt="preview" width="480">

Mixed-struct lowering repro: passing a struct whose fields are only partly numeric must qualify on the single numeric field.
