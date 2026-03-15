# Tuple Syntax and Type Inference

## Tuple Syntax

In Rust, `(a, b)` is term-level and `(A, B)` is type-level. Since we don't have syntactic distinction between type and term, we need to disambiguate.

### Option 1: Context-based (like Agda/Idris)

Same syntax in both positions—the typechecker uses context/expected type to disambiguate:
- In term position: pair term `(a, b)`
- In type position: sigma type `(A, B)`

### Option 2: Explicit syntax

Use different syntax:
- Term: `(a, b)`
- Type: `(A, B)` with explicit `Type` annotation

### Current Decision

Deferred. Tuples removed from prototype.

## Type Inference

Related question: when types cannot be inferred, how do we resolve ambiguity?

### Options

1. **Error on ambiguity** — Require explicit type annotations
3. **Bidirectional resolution** — Use expected type from context

### Current Decision

Deferred. Prototype uses explicit type annotations on `let` when needed.
