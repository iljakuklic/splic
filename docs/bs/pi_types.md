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

### Term Representation

```rust
// Variables use De Bruijn indices (count from nearest binder, 0 = innermost)
Term::Var(Ix)

// Pi types support variadic (multi-parameter) syntax
Pi { params: &'a [(&'a str, &'a Term<'a>)], body_ty: &'a Term<'a>, phase: Phase }

// Lambdas similarly support variadic parameters
Lam { params: &'a [(&'a str, &'a Term<'a>)], body: &'a Term<'a> }

// Application handles variadic calls
App { func: &'a Term<'a>, args: &'a [&'a Term<'a>] }

// Global references are terms, not application heads
Global(Name<'a>)
```

**Variadic Design.** Pi and Lam now carry a parameter list rather than single parameter. This preserves arity information and enables proper multi-argument application:
- `fn(x: u64, y: u64) -> u64` is a single Pi with 2 params, not nested Pi types.
- Application checking evaluates the domain type, checks the argument, then advances to the next param (via closure instantiation).

**Phase field.** The Pi carries a `phase: Phase` distinguishing meta-level (`Phase::Meta`, printed as `fn`) from object-level (`Phase::Object`, printed as `code fn`) function types.

### Substitution → Normalization by Evaluation (NbE)

**Removed:** Syntactic substitution (`fn subst(...)`) is **deleted**. It had a critical variable-capture bug when the replacement contained binders.

**New approach:** The type checker uses **Normalization by Evaluation** to handle dependent types. Instead of rewriting syntax, the checker maintains a **semantic domain** (`Value`) and evaluates types in context. Dependent function arguments are checked by:

1. Evaluating the Pi type in the current environment to obtain a semantic `VPi`.
2. Checking the argument against the evaluated domain type.
3. Instantiating the Pi's closure with the evaluated argument to get the return type.

See `docs/bs/nbe_and_debruijn.md` for complete details on the semantic domain and evaluation.

### Alpha-equivalence

Two terms are alpha-equivalent if they are structurally identical under De Bruijn indices (parameter names are irrelevant). The `alpha_eq` function in `core/alpha_eq.rs` performs structural comparison. With De Bruijn indices, this is a straightforward recursive check — renaming machinery is unnecessary.

## Type Checker NbE

### Semantic Domain (core/value.rs)

The type checker maintains semantic values separate from syntax to enable normalization:

```rust
pub enum Value<'a> {
    // Neutrals (cannot reduce)
    Rigid(Lvl),                          // local variable (De Bruijn level)
    Global(&'a str),                     // global function reference
    Prim(Prim),                          // primitive operation
    App(&'a Value<'a>, &'a [Value<'a>]), // application

    // Canonical forms
    Lit(u64),                            // literal
    Lam(VLam<'a>),                       // lambda with closure
    Pi(VPi<'a>),                         // Pi type with closure
    Lift(&'a Value<'a>),                 // lifted type
    Quote(&'a Value<'a>),                // quoted code
}

pub struct VLam<'a> {
    pub name: &'a str,
    pub param_ty: &'a Value<'a>,
    pub closure: Closure<'a>,
}

pub struct VPi<'a> {
    pub name: &'a str,
    pub domain: &'a Value<'a>,
    pub closure: Closure<'a>,
    pub phase: Phase,
}

pub struct Closure<'a> {
    pub env: &'a [Value<'a>],  // snapshot of evaluation environment
    pub body: &'a Term<'a>,    // unevaluated body term
}
```

### Key Operations

**`eval(arena, globals, env, term) -> Value`:** Interpret a term in an environment.
- `Var(Ix(i))`: index into `env[env.len() - 1 - i]` (convert index to stack position).
- `Lam` / `Pi`: create a closure by snapshotting `env` to the arena and pairing it with the body.
- Other forms: recursively evaluate or return as neutrals.

**`apply(arena, globals, closure, arg) -> Value`:** Instantiate a closure with an argument.
- Clone the closure's environment, push the argument, evaluate the body.

**`quote(arena, depth, value) -> &'a Term`:** Convert a value back to term syntax.
- `Rigid(lvl)`: convert level to index using `lvl_to_ix(depth, lvl)`.
- For `Lam` / `Pi`: apply the closure to a fresh variable, recursively quote the result.

### Dependent Type Checking

When checking a multi-argument application:

1. Evaluate the function's type to get `Value::Pi(vpi)`.
2. Check the first argument against `vpi.domain`.
3. Evaluate the argument to a value.
4. **Instantiate the Pi's closure** with the evaluated argument: `apply(closure, arg_value)` yields the type of remaining args or return type.
5. Repeat for each argument.

This replaces syntactic substitution and eliminates variable capture bugs.

### Distinction from Staging Evaluator

The type checker's NbE and the staging evaluator (`eval/mod.rs`) are separate:
- **Type checker NbE** uses a unified `Value` domain to normalize types during elaboration.
- **Staging evaluator** uses separate `Val0`/`Val1` domains to partition meta/object computation and produce object code.

Both use `Closure { env, body }` pattern for closures, but serve different purposes and cannot be unified.

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
