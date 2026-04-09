# WebAssembly Backend

Notes from initial design discussion for the planned WebAssembly codegen backend.

## Input

The backend consumes the output of `unstage_program`: a `Program` containing only
`code fn` definitions with a splice/quote-free `Term` IR. The IR nodes at this point
are: `Var`, `Lit`, `Prim`, `Global`, `App`, `Let`, `Match`.

## Type Mapping

Wasm's integer value types are `i32` and `i64`. Splic's integer widths map as follows:

| Splic | Wasm  | Notes                                      |
|-------|-------|--------------------------------------------|
| `u0`  | `i32` | Always the constant `0`; computation erased |
| `u1`  | `i32` | Mask with `0x1` after each op              |
| `u8`  | `i32` | Mask with `0xFF` after each op             |
| `u16` | `i32` | Mask with `0xFFFF` after each op           |
| `u32` | `i32` | Wasm `i32` wraps naturally                 |
| `u64` | `i64` | Wasm `i64` wraps naturally                 |

Sub-word types (`u1`, `u8`, `u16`) are widened to `i32` and truncated after each
operation via a bitwise AND mask. This naturally gives wrapping semantics.

The `Prim` variants already carry `IntType` (which includes `IntWidth`), so the
codegen function for each op has all information needed to emit the correct mask.

## u0 Erasure

`u0` is a zero-information type (one value: `0`). Rather than using a GC struct
reference (which would require the Wasm GC proposal and heap allocation), `u0` is
lowered to `i32` with the constant value `0`. Any expression of type `u0` emits
`i32.const 0`; the actual computation is skipped.

## Overflow / Wrapping Semantics

Both the Wasm backend and the meta-level evaluator use **wrapping arithmetic**,
matching Wasm's native `i32`/`i64` behavior. Sub-word types additionally mask the
result to their width, giving consistent wrap-at-width behavior across all integer
types.

This is a deliberate choice for phase coherence: `fn` and `code fn` with identical
bodies should behave identically, and phase-polymorphic functions (planned) must
have a single well-defined semantics regardless of instantiation phase. Overflow
errors at compile time but wrapping at runtime would violate this.

Division by zero remains an error at both levels — this is undefined behavior, not
overflow.

## Suggested Implementation Approach

1. Add `wasm-encoder` as a dependency in `compiler/Cargo.toml`.
2. Create `compiler/src/codegen/wasm.rs` with a `codegen_program(program: &Program) -> Vec<u8>` function.
3. Walk each `Function`, emit a typed Wasm function.
4. Walk `Term` recursively emitting stack instructions (expression trees map naturally onto Wasm's stack machine).
5. Handle object-level `Match` as `block` + `br_if` chains for now; revisit with `br_table` later.
6. Add a `cargo run -- compile --target wasm <FILE>` subcommand to the CLI.

## Open Questions

- **Loops**: object-level `Match` exists but there is no loop construct yet. Useful
  Wasm output for non-trivial programs will require object-level loops. See
  `functional_goto.md` for relevant design notes.
- **Product types / calling conventions**: how multi-value and product types map to
  Wasm params/results is deferred until product types are designed.
