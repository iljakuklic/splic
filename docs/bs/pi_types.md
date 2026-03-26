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
|x: A, y: B| body    // multi-parameter (desugars to nested lambdas)
```

Type annotations on lambda parameters are **mandatory**. This makes lambdas inferable — the typechecker can construct the full Pi type from the annotation and the inferred body type, without needing an expected type pushed down from context. This is a deliberate simplification for the prototype; unannotated `|x| body` syntax may be added later when check-mode lambdas are needed.

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

Application is inferable when the function is inferable:

```
Γ ⊢ f ⇒ fn(x: A) -> B    Γ ⊢ arg ⇐ A
─────────────────────────────────────────
       Γ ⊢ f(arg) ⇒ B[arg/x]
```

The return type `B[arg/x]` is the body type with the argument substituted for the parameter. For non-dependent functions this is just `B`.

Multi-argument calls desugar to curried application: `f(a, b)` = `f(a)(b)`.

## Core IR Design

### New Term variants

```rust
Pi { param_name: &'a str, param_ty: &'a Term<'a>, body_ty: &'a Term<'a> }
Lam { param_name: &'a str, param_ty: &'a Term<'a>, body: &'a Term<'a> }
FunApp { func: &'a Term<'a>, arg: &'a Term<'a> }
Global(Name<'a>)
PrimApp { prim: Prim, args: &'a [&'a Term<'a>] }
```

### Refactoring App/Head

The current `App { head: Head, args }` where `Head` is `Global(Name) | Prim(Prim)` is replaced by:

- **`Global(Name)`** — a term representing a reference to a top-level function. Now a first-class term rather than just an application head.
- **`FunApp { func, arg }`** — single-argument curried application. Used for both global and local function calls. Multi-arg calls `foo(a, b)` elaborate to `FunApp(FunApp(Global("foo"), a), b)`.
- **`PrimApp { prim, args }`** — primitive operation application. Kept separate because prims carry resolved `IntType` and are always fully applied. Eventually prims will become regular typed symbols, but the typechecker isn't ready for that yet.

**`FunSig` is preserved** as a convenience structure in the globals table. It stores the flat parameter list and return type for efficient lookup. A `FunSig::to_pi_type(arena)` method constructs the corresponding nested Pi type when needed (e.g., for `type_of(Global(name))`).

### Substitution

Dependent return types require substitution: `B[arg/x]`. Since the core IR uses De Bruijn levels, substitution replaces `Var(lvl)` with the argument term. Levels do not shift, making the implementation straightforward:

```rust
fn subst<'a>(arena: &'a Bump, term: &'a Term<'a>, lvl: Lvl, replacement: &'a Term<'a>) -> &'a Term<'a>
```

### Alpha-equivalence

The current `PartialEq` on `Term` compares structurally, including `param_name` fields. Two Pi types that differ only in parameter names (`fn(x: A) -> B` vs `fn(y: A) -> B`) should be equal. A dedicated `alpha_eq` function ignores names and compares only structure (De Bruijn levels handle binding correctly).

## Evaluator Design

### Closures

A new `MetaVal` variant captures the environment at lambda creation:

```rust
VClosure {
    param_name: &str,
    body: &Term,
    env: Vec<Binding>,
    obj_next: Lvl,
}
```

This follows the substitution-based approach already in use. Application extends the captured env with the argument value and evaluates the body.

### Global function references

When `eval_meta` encounters `Global(name)`, it constructs a closure from the global's body and parameters. When applied via `FunApp`, this closure behaves identically to a lambda — the argument extends the env and the body is evaluated.

For multi-parameter globals, partial application produces a closure that awaits the remaining arguments. This falls out naturally from curried `FunApp` chains.

### Pi types in evaluation

`Pi` terms are type-level and never appear in evaluation position (the typechecker ensures this). They are unreachable in `eval_meta`.

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
fn const_(A: Type, B: Type) -> fn(A) -> fn(B) -> A {
    |a: A| |b: B| a
}
```

### Function composition

```splic
fn compose(A: Type, B: Type, C: Type, f: fn(B) -> C, g: fn(A) -> B) -> fn(A) -> C {
    |x: A| f(g(x))
}
```

### Higher-order staging

```splic
fn map_code(f: fn([[u64]]) -> [[u64]], x: [[u64]]) -> [[u64]] {
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
