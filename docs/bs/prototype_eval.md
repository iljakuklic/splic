# Prototype Evaluator Design

## Overview

This document records the design decisions for implementing the meta-level evaluator
in Splic, which powers staging (eliminating splices from mixed-stage programs). It
also outlines the progression to spine-based evaluation (for dependent types) and
dependent types themselves.

The discussion and rationale are documented here to preserve the design space
explored and decisions made.

---

## Key Context

Splic is a 2LTT language with two phases:
- **Meta level** (compile time): full computation, recursion, real β-reduction
- **Object level** (runtime): code (opaque at compile time, except via quotation)

The object level is deliberately treated with *weak* definitional equality:
- No β-reduction for object lambdas
- No let-unfolding
- Just structural/α-equivalence

This keeps object typechecking simple; object code is *data* during meta computation.

---

## Design Decisions

### 1. Staging vs Dependent Types — Staging First ✅

**Decision**: Implement staging and unstaging *before* adding dependent types.

**Rationale**:
- Staging is simpler (no unification, no dependent elimination, no normalizer).
- Splic's core feature is metaprogramming for zkVMs; staging is semantically fundamental.
- The "hello world" power example uses staging but no dependent types.
- Once staging works, you have a concrete output IR (splice-free object programs) that dependent types can build on.
- Dependent types will eventually need a normalizer anyway (for dependent meta types).

**Timeline**: Staging first, dependent types later.

---

### 2. Object-Level Definitional Equality — Weak ✅

**Decision**: Object definitional equality has no β-reduction or let-unfolding.

**Rationale**:
- Philosophically cleaner: object code is code, not computation.
- Keeps object typechecking simple even though object terms are examined at compile time via quotation.
- Matches the reference implementation (Kovács 2022 / 2024).
- When dependent types arrive and you need a normalizer for dependent object types, weak equality means the normalizer can be *syntactic* (structural/α) rather than semantic.
- For zkVM targeting, object code should be opaque to the meta evaluator except through explicit quotation.

**Implication**: When the meta evaluator encounters object code, it treats it as inert data (no reduction).

---

### 3. Environment Design — Unified ✅

**Decision**: Use a single unified environment for both meta and object bindings.

```rust
enum Binding<'a> {
    Meta(MetaVal<'a>),
    Obj(Lvl),           // object variable tracks its De Bruijn level in the output
}
```

Variables are indexed by De Bruijn level (oldest binding first); variable names are not stored in the environment — they are only kept in terms for pretty-printing.

**Rationale**:
- Cleaner when meta needs to reference object bindings via quotation (single lookup).
- Simpler code overall (one environment management path).
- Mirrors how the typechecker already works (one locals stack with phase tracking).
- Enables future automatic quotation/splice inference (typechecker can infer stage crossings without explicit syntax).

---

### 4. Lambda Evaluation Strategy — NbE with Environment Closures ✅

**Decision**: Use Normalization by Evaluation (NbE) with environment-captured closures throughout, for both the type checker and the staging evaluator.

**Actual approach (NbE)**:
- Lambdas and Pi types evaluate to semantic values carrying a snapshot of the environment at creation time (closures).
- Application extends the captured environment with argument values and re-evaluates the body.
- `quote` converts semantic values back to terms by introducing fresh rigid variables.
- Definitional equality is checked by quoting both sides and comparing structurally (α-equivalence).

This approach was chosen over the substitution-first → spine-based path originally planned because:
- NbE is well-suited to the dependent type checking needed for Pi types.
- It handles all three concerns (staging evaluator, type checker, definitional equality) with a single uniform mechanism.
- The multi-param variadic design (see `pi_types.md`) fits naturally: domain closures for each parameter share a base environment snapshot and accumulate argument values incrementally.

**Spine-based evaluation** remains an option for future optimization (lazy forcing under binders, more efficient unification), but is not currently required.

---

### 5. IR Structure — Unified Term Type ✅

**Decision**: Keep the elaborated IR as a unified `Term` type (as currently implemented).

**Rationale**:
- Phase checking happens at elaboration time; phase invariants are enforced then.
- Evaluators (`eval_meta`, `eval_obj`) take a `&Term` and produce phase-specific values.
- Matches the reference impl's approach (unified through elaboration, split in values).
- No IR duplication; simpler to maintain.
- Example:
  ```rust
  fn eval_meta(arena: &Bump, env: &Env, term: &Term) -> MetaVal { ... }
  fn eval_obj(arena: &Bump, env: &Env, term: &Term) -> ObjVal { ... }
  ```

---

### 6. Literal Type Annotation — Defer Until Normalizer ✅

**Decision**: During elaboration, the type of literals is provided by context. Type recovery from
syntax alone is deferred until the normalizer is implemented (when dependent types are added).

**Rationale**:
- During elaboration, literals are checked against an expected type provided by the caller.
- Most term variants carry enough information for type recovery (self-typed); literals are the exception.
- Once NbE is implemented and types become semantic values, the need to recover types from syntax diminishes.
- Adds no runtime cost (the type information is already in hand during elaboration).

**Implementation**: Currently handled via context-threaded types. Will be revisited if IR redesign
is needed for dependent types.

---

## Implementation Sequence

The original plan described three phases. The actual path differed: Phase 2 (spine refactor) was skipped; the implementation went directly to NbE with environment closures, which subsumes both Phase 1 and Phase 2.

1. **Phase 1: Staging evaluator** ✅
   - Implemented `eval_meta`, unified `Env`, `unstage_program` entry point.
   - Test corpus: snapshots of staged programs.

2. **Phase 2: Spine-based refactor** — *skipped*
   - Original plan was to refactor to spines before dependent types.
   - Instead, NbE with closures was implemented directly (see Decision 4), which handles dependent types without a separate refactor step.

3. **Phase 3: Dependent Pi types** ✅
   - NbE type checker with `eval` / `quote` / definitional equality.
   - Multi-param variadic Pi and Lam (no currying; strict arity checking).
   - Dependent telescopes via domain closures.
   - See `pi_types.md` for the design.

**Remaining future work**: spine-based evaluation (optimization), implicit arguments, object-level closures. See `prototype_next.md`.

---

## References

- **Pi Types Design**: [pi_types.md](pi_types.md)
- **Reference Implementation**: https://github.com/AndrasKovacs/staged
- **Kovács 2022**: Staged Compilation with Two-Level Type Theory (ICFP)
- **Kovács 2024**: Closure-Free Functional Programming in a Two-Level Type Theory
