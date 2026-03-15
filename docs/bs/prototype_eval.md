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
    Obj(ObjVal<'a>),
}

struct Env<'a> {
    vars: Vec<(&'a str, Binding<'a>)>,  // indexed by De Bruijn level
}
```

**Rationale**:
- Cleaner when meta needs to reference object bindings via quotation (single lookup).
- Simpler code overall (one environment management path).
- Mirrors how the typechecker already works (one locals stack with phase tracking).
- Enables future automatic quotation/splice inference (typechecker can infer stage crossings without explicit syntax).

---

### 4. Lambda Evaluation Strategy — Substitution First, Spines Later ✅

**Decision**: Use explicit substitution (Option 1) for the initial prototype. Refactor to
spine-based evaluation (Option 3) before introducing dependent types.

**Initial approach (substitution)**:
```rust
enum MetaVal<'a> {
    VVar(Lvl),
    VLam(&'a str, &'a MetaTm<'a>),  // unevaluated body
    VCode(&'a ObjVal<'a>),
    // ... literals, prims, etc.
}

fn eval_meta_app(func: MetaVal, arg: MetaVal, env: &Env, arena: &Bump) -> MetaVal {
    match func {
        MetaVal::VLam(name, body) => {
            // extend env with arg, re-evaluate body
            let extended_env = env.extend(arena, name, arg);
            eval_meta(arena, &extended_env, body)
        }
        // ... other cases
    }
}
```

**Rationale for starting with substitution**:
- Simplest to implement correctly and test.
- Easy to debug and reason about.
- Fits naturally with arena allocation (extend env, re-eval).
- Good enough for initial staging without optimization.
- Allows rapid prototyping and testing of staging semantics.

**Why refactor to spines before dependent types**:
- With dependent types, you have dependent elimination (fold, recursor, etc.) that evaluates under binders.
- Re-evaluation on each application becomes painful in that context (redundant work, no memoization).
- Spine-based evaluation (lazy, tracking pending operations) is the *proven* approach in dependently typed languages (Agda, Lean, etc.).
- Unification with spines is standard and more efficient.
- If you refactor later, you're refactoring with an existing test corpus and staging semantics already validated.

**Spine-based approach (future)**:
```rust
enum MetaSpine<'a> {
    SId,
    SApp(Box<MetaSpine<'a>>, MetaVal<'a>),
}

enum MetaVal<'a> {
    VVar(Lvl, MetaSpine<'a>),        // stuck variable + pending ops
    VLam(&'a str, &'a MetaTm<'a>),   // lambda (unevaluated)
    // ... other values
}
```

When applying a lambda, push the argument onto a spine; only force evaluation when needed.

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

**Decision**: Leave `Lit(u64)` as-is for now. Add width annotation (`Lit(u64, IntType)`)
when implementing the normalizer for dependent types.

**Rationale**:
- Currently not needed; `check` provides the expected type.
- When a `type_of(term) -> Value` function is needed (for the normalizer), `Lit` is one of the few variants that can't self-type.
- The change is minimal (one line to IR, one line to elaborator).
- The `IntType` is already in hand at the elaboration site (it's in the `expected` type passed to `check`), so adding it costs nothing.

**Timeline**: Add after refactoring to spines, before dependent types.

---

## Implementation Sequence

See [prototype_next.md](prototype_next.md) for the full roadmap, but the evaluator-specific sequence is:

1. **Phase 1: Substitution-based evaluator + staging**
   - Implement `eval_meta`, `eval_obj`, unified `Env`.
   - Implement `unstage` entry point: `eval_obj(arena, Env::empty(), program) -> Term` (splice-free).
   - Test corpus: snapshots of staged programs (e.g., `repeat` example).
   - Goal: Staging works; splice elimination is validated.

2. **Phase 2: Refactor to spine-based evaluation**
   - Restructure evaluator to use spines; lazy application.
   - Implement `force` / `quote` for re-normalization when needed.
   - All existing tests should still pass (change is internal to evaluator).
   - Goal: Prepared for dependent types; potential performance improvements.

3. **Phase 3: Introduce dependent types**
   - Add `Π` (dependent function type).
   - Implement dependent elimination (fold, recursor, or general elimination).
   - Implement unification with spines.
   - Add normalizer for conversion checking (using spine-based evaluator).
   - Annotate `Lit` with `IntType` (now required for `type_of`).
   - Goal: Full dependent meta level.

---

## References

- **2LTT Overview**: [../../.opencode/skills/2ltt/implementation-guide.md](../../.opencode/skills/2ltt/implementation-guide.md)
- **Reference Implementation**: https://github.com/AndrasKovacs/staged
- **Kovács 2022**: Staged Compilation with Two-Level Type Theory (ICFP)
- **Kovács 2024**: Closure-Free Functional Programming in a Two-Level Type Theory
