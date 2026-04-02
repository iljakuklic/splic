# Pi Types: Dependent Function Types at the Meta Level

This document records the design decisions for adding dependent function types (Pi types) and lambda abstractions to Splic at the meta level.

## Motivation

The current prototype has no first-class functions. All functions are top-level named definitions; there is no way to pass a function as an argument or return one from another function. This blocks key use cases:

**Higher-order meta functions.** The `repeat` combinator from `prototype_next.md` requires passing a code-generating function:

```splic
fn repeat(f: fn([[u64]]) -> [[u64]], n: u64, x: [[u64]]) -> [[u64]] {
    match n {
        0 => x,
        n => repeat(f, n - 1, f(x)),
    }
}

code fn square_twice(x: u64) -> u64 {
    $(repeat(|y: [[u64]]| #($(y) * $(y)), 2, #(x)))
}
```

**Polymorphic functions.** Dependent function types let parameters appear in subsequent types:

```splic
fn id(A: Type, x: A) -> A { x }
```

Here `A` is a type passed at compile time, and the return type depends on it.

**Type-level computation.** With Pi types, function types are first-class terms in the meta universe, enabling functions that compute types.

## Syntax

### Function types

Dependent function types use the `fn` keyword — the same keyword used for definitions:

```
fn(x: A) -> B       // dependent: B may mention x
fn(_: A) -> B        // non-dependent: wildcard name required
```

Right-associative: `fn(_: A) -> fn(_: B) -> C` means `fn(_: A) -> (fn(_: B) -> C)`.

Multi-parameter function types are **not** desugared to nested Pi — the arity is preserved to enable proper arity checking at call sites:

```
fn(x: A, y: B) -> C   -- two-argument function, not sugar for nested Pi
```

**Rationale.** Using `fn` for types mirrors its use for definitions — in Splic, `fn` introduces anything function-shaped. The parenthesized parameter syntax `fn(x: A)` is visually distinct from a definition `fn name(x: A)` (the presence of a name between `fn` and `(` distinguishes them). The `(x: A) -> B` Agda/Lean convention was considered but `fn(x: A) -> B` is more Rust-flavored.

### Lambdas

Lambda expressions use Rust's closure syntax:

```
|x: A| body          // type annotation required
|x: A, y: B| body    // multi-parameter
|| body              // nullary: produces a fn() -> T value
```

Type annotations on lambda parameters are **mandatory**. This makes lambdas inferable — the typechecker can construct the full Pi type from the annotation and the inferred body type, without needing an expected type pushed down from context. Check-mode (unannotated) lambdas are also supported when the expected Pi type is known from context.

**Rationale.** The `|...|` syntax is familiar to Rust users. It reuses the existing `|` token. Disambiguation with bitwise OR is positional: `|` at the start of an atom is a lambda; `|` after an expression is bitwise OR.

### Scope

Pi types and lambdas are **meta-level only**. Object-level functions remain top-level `code fn` definitions. A lambda cannot appear in object-level code, and `fn(_: A) -> B` cannot appear as an object-level type. This matches the 2LTT philosophy: the meta level is a rich functional language; the object level is a simple low-level language.

## Typing Rules

Pi types inhabit the meta universe (`Type`). The formation, introduction, and elimination rules:

### Formation (Pi)

```
Γ ⊢ A : Type    Γ, x : A ⊢ B : Type
──────────────────────────────────────
         Γ ⊢ fn(x: A) -> B : Type
```

Both `A` and `B` must be types. The parameter `x` is in scope in `B` (dependent case). For non-dependent arrows, `x` does not appear free in `B`.

### Introduction (Lambda)

Lambdas are **inferable** because type annotations on parameters are mandatory:

```
Γ ⊢ A : Type    Γ, x : A ⊢ body ⇒ B
─────────────────────────────────────────
   Γ ⊢ |x: A| body ⇒ fn(x: A) -> B
```

The parameter type `A` comes from the annotation; the body type `B` is inferred in the extended context. The synthesised type is the Pi type `fn(x: A) -> B`.

### Elimination (Application)

Application is inferable when the function is inferable. For a call `f(a₁, ..., aₙ)`, the number of arguments must **exactly match** the arity of the callee's Pi type:

```
Γ ⊢ f ⇒ fn(x₁: A₁, ..., xₙ: Aₙ) -> B    Γ ⊢ aᵢ ⇐ Aᵢ[a₁/x₁, ..., aᵢ₋₁/xᵢ₋₁]
──────────────────────────────────────────────────────────────────────────────────
                  Γ ⊢ f(a₁, ..., aₙ) ⇒ B[a₁/x₁, ..., aₙ/xₙ]
```

Each argument is checked against its domain, which may depend on prior arguments (supporting dependent telescopes). The return type has all parameters substituted. For non-dependent functions the types simplify to plain `B`.

**No partial application.** `f(a)` on a two-argument function `fn(A, B) -> C` is a type error — the arity must match exactly. To partially apply, the programmer must write an explicit eta-expansion: `|b: B| f(a, b)`.

**Nullary functions.** `fn() -> T` is a distinct type from `T`. A value `f: fn() -> T` must be called explicitly with `f()` to produce a `T`. A bare reference to `f` does not evaluate the body.

## Core IR Design

### Term Representation

**Variadic design (divergence from Kovacs).** In the Kovacs 2022 reference implementation, multi-param Pi types are represented as iterated single-param Pi types (currying): `fn(A, B) -> C` is stored as `fn(A) -> fn(B) -> C`. Splic diverges from this: Pi and Lam carry a parameter list, preserving the original arity:

- `fn(x: u64, y: u64) -> u64` is a single Pi with 2 params, not two nested single-param Pi types.
- `fn() -> T` is a Pi with 0 params, distinct from `T`.
- Arity checking at call sites is strict: the argument list must have exactly as many elements as the Pi's parameter list.
- Dependent telescopes are supported: the type of each parameter may depend on the values of preceding parameters.

This design makes arity errors detectable without any runtime information, and avoids the ambiguity between `fn(A) -> fn(B) -> C` and `fn(A, B) -> C` that would arise from currying.

**Phase field.** Pi types carry a phase distinguishing meta-level (printed as `fn`) from object-level (printed as `code fn`) function types.

### Substitution → Normalization by Evaluation (NbE)

Syntactic substitution is avoided due to capture bugs. Instead, the type checker uses **Normalization by Evaluation**: it evaluates types in a semantic domain (environment of values) and handles dependent type checking via closure instantiation. See `nbe_and_debruijn.md` for details.

### Alpha-equivalence

Two terms are alpha-equivalent if they are structurally identical under De Bruijn indices (parameter names are irrelevant). With De Bruijn indices, equivalence checking is a straightforward recursive check — no renaming machinery is needed.

## Staging Interaction

### Closures cannot be quoted

Meta-level closures (`VClosure`) cannot appear in object-level code. The type system prevents this:

- Pi types inhabit `Type` (meta universe), not `VmType` (object universe)
- Therefore `[[fn(A) -> B]]` is ill-formed — you cannot lift a function type
- Lambdas have Pi types, so they cannot have lifted types, so they cannot be quoted

This is the correct behavior: closures are compile-time values that are fully evaluated during staging.

### Code-generating lambdas

The main staging use case is lambdas that *produce* code:

```splic
fn repeat(f: fn([[u64]]) -> [[u64]], n: u64, x: [[u64]]) -> [[u64]] {
    match n {
        0 => x,
        n => repeat(f, n - 1, f(x)),
    }
}

code fn square_twice(x: u64) -> u64 {
    $(repeat(|y: [[u64]]| #($(y) * $(y)), 2, #(x)))
}
```

Here `f` has type `fn([[u64]]) -> [[u64]]` — it takes object code and returns object code. The lambda `|y| #($(y) * $(y))` is a meta-level function that generates object-level multiplication code. After staging, all meta computation (including the lambda and the `repeat` recursion) is erased:

```splic
code fn square_twice(x: u64) -> u64 {
    (x * x) * (x * x)
}
```

### Object-level FunApp/Global

`FunApp` and `Global` can appear in object-level terms for object-level function calls. The unstager passes them through structurally (copying to the output arena), just as it does for the current `App { head: Global }`.

## Examples

### Polymorphic identity

```splic
fn id(A: Type, x: A) -> A { x }
fn use_id() -> u64 { id(u64, 42) }
```

### Const combinator

```splic
fn const_(A: Type, B: Type) -> fn(_: A) -> fn(_: B) -> A {
    |a: A| |b: B| a
}
```

### Function composition

```splic
fn compose(A: Type, B: Type, C: Type, f: fn(_: B) -> C, g: fn(_: A) -> B) -> fn(_: A) -> C {
    |x: A| f(g(x))
}
```

### Higher-order staging

```splic
fn map_code(f: fn(_: [[u64]]) -> [[u64]], x: [[u64]]) -> [[u64]] {
    f(x)
}

code fn double(x: u64) -> u64 {
    $(map_code(|y: [[u64]]| #($(y) + $(y)), #(x)))
}
// Stages to: code fn double(x: u64) -> u64 { x + x }
```

## Future Work

- **Prims as typed symbols**: Currently prims are special-cased with `PrimApp`. Eventually they should have types (polymorphic in width/phase) and be typechecked uniformly.
- **Object-level closures**: The closure-free approach from Kovács 2024 avoids runtime closures while still supporting higher-order object code.
- **Implicit arguments**: `fn {A: Type}(x: A) -> A` with unification to infer `A` at call sites.
- **Spine-based evaluation**: Replace substitution-based closures with lazy spines before adding full dependent elimination.

## References

- Kovács 2022: Staged Compilation with Two-Level Type Theory (ICFP)
- Kovács 2024: Closure-Free Functional Programming in a Two-Level Type Theory (ICFP)
- [prototype_eval.md](prototype_eval.md): Evaluator design and progression plan
- [prototype_next.md](prototype_next.md): Roadmap (Phase 2: Meta-level Functions, Phase 3: Dependent Types)
