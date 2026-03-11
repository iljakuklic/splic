# Prototype Core IR Design

This document records the design decisions made for the type checker and core IR of the Splic prototype. See `docs/prototype.md` for the prototype scope.

## Design Decisions

### Variable Representation

Variables in the core IR use **De Bruijn levels** (counting from the outermost binder). Levels are preferred over indices because extending the context never requires shifting existing terms. The elaboration context is a `Vec` indexed by level; lookup is a linear scan by name (sufficient for prototype-scale functions).

### Name Resolution

Name resolution is **integrated into the type checker** rather than being a separate pass. The elaborator maintains a context mapping source names to levels and types, resolving `Var(Name)` in the surface AST to `Var(Lvl)` in the core IR on the fly.

### Core IR vs. Parser AST

The core IR lives in a **separate `compiler/src/core/` module** with its own types. The elaborator translates `parser::ast::Term` → `core::Term` while type-checking. This avoids lifetime entanglement with the source string and provides a clean foundation for the unstager.

### Memory Allocation

The core IR follows the **same arena allocation strategy as the parser**: a `bumpalo::Bump` arena, `&'a T` references instead of `Box<T>`, and `&'a [T]` slices instead of `Vec<T>`. The checker struct holds `arena: &'a bumpalo::Bump`.

### Type Equality

Types are compared **structurally**. No hash-consing or interning. A pointer-equality fast-path (`std::ptr::eq`) can short-circuit comparison when the same arena allocation is referenced from two places, without the complexity of a full interning table.

### Types and Terms

Types and terms are **unified** in a single `Term` enum (types are terms). Universe kinds `Type` and `VmType` are `Term::Prim(Prim::U(Phase))` constructors.

### Function Types (`Pi`)

`Pi` is **not included** in the prototype core IR. Top-level functions are not first-class values; they live only in the globals table as `FunSig` records. Call sites check arguments positionally against the signature. When the language grows to need `[[T -> U]]` or first-class functions, `Pi` will be added together with promoting `App`'s head from `Head` to `Term` — a single coherent step.

### Forward References and Recursion

Top-level function definitions support **forward references and mutual recursion** via a two-pass approach: signatures (param types + return type) are collected in pass 1; bodies are elaborated in pass 2 with the full signature table available. This is feasible because type annotations are required on all top-level function signatures.

### Primitive Operations

A single `Prim` enum covers both **builtin types** (`u0`, `u8`, `u16`, `u32`, `u64`, `Type`, `VmType`) and **builtin operations** (`+`, `-`, `*`, `/`, `&`, `|`, `!`, `==`, `!=`, `<`, `>`, `<=`, `>=`). Operations carry a resolved `IntWidth` tag so a backend has all the information it needs to emit the correct instruction without further analysis.

---

## Core IR Types

```rust
// Phase::Meta = compile-time (meta level)
// Phase::Object = run-time (object level)
// Re-exported from parser::ast
pub use crate::parser::ast::Phase;

/// Integer widths for primitive types and operations
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntWidth { U0, U1, U8, U16, U32, U64 }

/// Built-in types and operations, fully resolved by the elaborator
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Prim {
    // Types (inhabit VmType at object phase, Type at meta phase)
    IntTy(IntWidth),
    // Universe: U(Meta) = Type, U(Object) = VmType
    U(Phase),
    // Arithmetic (binary)
    Add(IntWidth), Sub(IntWidth), Mul(IntWidth), Div(IntWidth),
    // Bitwise
    BitAnd(IntWidth), BitOr(IntWidth), BitNot(IntWidth),
    // Comparison (return U1)
    Eq(IntWidth), Ne(IntWidth),
    Lt(IntWidth), Gt(IntWidth), Le(IntWidth), Ge(IntWidth),
}

/// De Bruijn level (counts from the outermost binder)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Lvl(pub usize);

/// Head of an application: either a top-level function or a primitive op
#[derive(Clone, Copy, Debug)]
pub enum Head<'a> {
    Global(&'a str),  // resolved top-level function name
    Prim(Prim),       // built-in operation with resolved width
}

/// Match pattern in the core IR
#[derive(Debug)]
pub enum Pat<'a> {
    Lit(u64),
    Bind(Option<&'a str>),  // None = wildcard, Some(name) = named binding
}

/// Match arm
#[derive(Debug)]
pub struct Arm<'a> {
    pub pat:  Pat<'a>,
    pub body: &'a Term<'a>,
}

/// Top-level function signature (stored in the globals table during elaboration)
#[derive(Debug)]
pub struct FunSig<'a> {
    pub params:  &'a [(&'a str, &'a Term<'a>)],  // (name, type) pairs
    pub ret_ty:  &'a Term<'a>,
    pub phase:   Phase,
}

/// Elaborated top-level function definition
#[derive(Debug)]
pub struct Function<'a> {
    pub name: &'a str,
    pub sig:  FunSig<'a>,
    pub body: &'a Term<'a>,
}

/// Elaborated program: a sequence of top-level function definitions
#[derive(Debug)]
pub struct Program<'a> {
    pub functions: &'a [Function<'a>],
}

/// Core term / type (terms and types are unified)
#[derive(Debug)]
pub enum Term<'a> {
    /// Local variable, identified by De Bruijn level
    Var(Lvl),
    /// Built-in type or operation
    Prim(Prim),
    /// Numeric literal
    Lit(u64),
    /// Application of a global function or primitive operation to arguments
    App { head: Head<'a>, args: &'a [&'a Term<'a>] },
    /// Lift: [[T]] — meta type representing object-level code of type T
    Lift(&'a Term<'a>),
    /// Quotation: #(t) — produce object-level code from a meta expression
    Quote(&'a Term<'a>),
    /// Splice: $(t) — run meta code and insert result into object context
    Splice(&'a Term<'a>),
    /// Let binding with explicit type annotation and a body
    Let { name: &'a str, ty: &'a Term<'a>, expr: &'a Term<'a>, body: &'a Term<'a> },
    /// Pattern match
    Match { scrutinee: &'a Term<'a>, arms: &'a [Arm<'a>] },
}
```

---

## Module Layout

```
compiler/src/
├── lib.rs               (pub mod core; pub mod checker;)
├── core/
│   └── mod.rs           (Term, Prim, IntWidth, Head, Arm, Pat, FunSig, Lvl)
└── checker/
    ├── mod.rs           (Ctx, elaborate_program, check, infer)
    └── test/
        └── mod.rs
```

## Checker Structure

```
Pass 1 — collect_signatures:
  Walk Program, elaborate each function's param types and return type only.
  Populate the globals table (HashMap<&str, FunSig>).

Pass 2 — elaborate_bodies:
  For each Function, elaborate the body with the full globals table available.
  Locals context starts empty and is extended as let-bindings and match arms are entered.

Core operations:
  elaborate_program(arena, surface_program) -> Result<core::Program<'a>>
  infer(ctx, surface_term) -> Result<(&'a Term<'a>, &'a Term<'a>)>   // (elaborated term, its type)
  check(ctx, surface_term, expected_type) -> Result<&'a Term<'a>>    // elaborated term
```
