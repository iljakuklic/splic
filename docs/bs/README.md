# BS

This folder contains half-baked, incomplete feature ideas and proposals.
Nothing in here is guaranteed or even likely to get implemented.
It serves mostly as a record of evolution of how various features come about
with discussion of trade-offs considered.

The folder name, `bs`, stands for brainstorming. Obviously.

## Language Design

- [defs_syntax.md](defs_syntax.md) — Proposed unified definition syntax (`def`, `let`, `lam`, `fn`)
- [functional_goto.md](functional_goto.md) — Control flow via SSA-style basic blocks with goto
- [comparison_operators.md](comparison_operators.md) — Boolean vs propositional comparisons
- [tuples_and_inference.md](tuples_and_inference.md) — Tuple syntax and type inference

## Compiler Internals

- [prototype_core.md](prototype_core.md) — Prototype core IR design decisions
- [self_typed_ir.md](self_typed_ir.md) — Self-typed core IR and a future `type_of` method
- [prototype_eval.md](prototype_eval.md) — Evaluator design and implementation sequence (substitution → spines → dependent types)
- [nbe_and_debruijn.md](nbe_and_debruijn.md) — Normalization by Evaluation, De Bruijn indices vs levels, free variable index shifting in staging
- [pi_types.md](pi_types.md) — Dependent function types (Pi) and lambdas at the meta level (implementation details and NbE type checking)
- [wasm_backend.md](wasm_backend.md) — WebAssembly backend design: type mapping, u0 erasure, wrapping semantics

## Roadmap, Strategy & Process

- [prototype_next.md](prototype_next.md) — Next steps after basic prototype (phases: staging, meta functions, dependent types)
- [quality.md](quality.md) — Clippy lint philosophy and workflow
