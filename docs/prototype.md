# Prototype Specification

This document specifies a minimal demo prototype for Splic, enough to compile a basic power function with a compile-time exponent.

## Motivation

The goal is to verify that two-level type theory (2LTT) can work in practice for zkVM code generation. We want a minimal working example that demonstrates:

1. Meta-level computation (running at compile time)
2. Object-level code generation via quotations
3. Splicing computed code into the output

The power function is an ideal test case: it has a loop (requires object-level control flow), the exponent is known at compile time (enables significant optimization), and the result is demonstrably different from naive implementation.

## Target Example

### User-facing syntax

```splic
fn power(exp: u64, x: code(u64)) -> code(u64) {
    match exp {
        0 => #(1),
        1 => x,
        exp => {
            let exp2 = #{
                let x2 = $(power(exp / 2, x));
                x2 * x2
            };
            match (exp & 1) == 1 {
                0 => exp2,
                1 => #(exp2 * $x),
            }
        }
    }
}

fn code pow5(x: u64) -> u64 {
    $(power(5, #(x)))
}
```

### Expected expansion

```splic
fn code pow5(x: u64) -> u64 {
    let x2 = x * x;
    let x4 = x2 * x2;
    let x5 = x4 * x;
    x5
}
```

Note how the recursive call to `power` with compile-time exponent 5 gets fully unrolled into straight-line code.

## Syntax Constructs

### Quotations and splices

- `#(expr)` — produces object-level code from a meta-level expression
- `#{ stmts }` — produces object-level code from a block (equivalent to `#({ stmts })`)
- `$e` — splices a meta-level expression producing object-level code into surrounding object-level context
- `code(T)` — type representing object-level code of type T (lifting)

The `$` syntax mimics Rust macros, which should feel familiar. The `#` syntax is concise and extensible.

### Functions

- `fn foo() -> T` — meta-level function (default)
- `fn code foo() -> T` — object-level function

The `code` keyword explicitly marks object-level functions. This is temporary—we expect to infer this from context once phase polymorphism is better understood.

## Primitive Types

| Type   | Description      |
|--------|------------------|
| `u0`   | Unit type        |
| `u1`   | Boolean          |
| `u8`   | 8-bit unsigned   |
| `u16`  | 16-bit unsigned  |
| `u32`  | 32-bit unsigned  |
| `u64`  | 64-bit unsigned  |

### Primitive Operations

- Arithmetic: `+`, `-`, `*`, `/`
- Comparison: `==`, `!=`, `<`, `>`, `<=`, `>=`
- Bitwise: `~`, `&`, `|`

### Tuples

Syntax: `(a, b)`. Only 2-tuples in the prototype, N-tuples achieved by nesting.

## Language Constructs

### Let binding

```
let x = e1 in e2
```

No pattern matching in let for the prototype—just simple variable binding.

### Match

```
match e {
    pat => e,
    ...
}
```

Requirements:
- Exhaustive: must cover all cases or include a match-all (`_`, `x`)
- No nested matching for the prototype

## Type System

### Universes

Two separate universes:
- `Type` — meta-level types
- `VmType` — object-level types

Both are type-in-type for now (no universe hierarchy). This simplifies the prototype significantly.

### Lifting

- `Lift A` — meta-level type representing object-level code of type A
- `code` keyword is sugar for `Lift`

### Bidirectional Type Checking

The typechecker is syntax-directed with no unification or constraint solving:

- **Checking mode**: Expected type provided, verify term matches
- **Inference mode**: No expected type, synthesize type from term structure

Type annotations are required on function signatures, but the body can infer types for:
- Bound variables (from lambda/let context)
- Return types (from body)
- Application arguments (from function type)

No implicit arguments are supported—all arguments are explicit.

### Type Annotations

Users write types on:
- Function parameters and return types
- Let-bound variables (optional if inferable)

The body expressions are inferred.

## Rationale

- **`code` keyword**: Explicit marking avoids phase inference complexity until patterns become clear.
- **No implicit arguments**: Skips Agda-style unification with pruning, eta-expansion, flexible/rigid spines.
- **Type-in-type**: Simpler than cumulative universe hierarchy—just need `Type` and `VmType`.
- **Bidirection without unification**: Syntax-directed only, no constraint solving—enough for prototype.
- **Separate universes**: Makes stage explicit at type level, easier to reason about than per-value stages.

## Implementation Omissions

The following are explicitly NOT included in the prototype:

- User-defined types / ADTs
- Dependent types (though syntax should not conflict)
- Object-level control flow constructs (while, loops, goto)
- Effects / effect handling
- Implicit parameters
- Phase polymorphism inference
- Elaborate error messages (basic errors only)

## Future Considerations

These features may be added after the prototype validates the core approach:

- Full dependent types (indexed types, proofs)
- Type inference with unification
- Implicit arguments
- GADTs / pattern matching on types
- Object-level control flow (functional goto, structured)
- Closure-free object language (as per Kovács 2024)
- Kind-level polymorphism (generics without dedicated syntax)

## References

- Kovács 2022: Staged Compilation with Two-Level Type Theory
- Kovács 2024: Closure-Free Functional Programming in a Two-Level Type Theory
- Splic concept: docs/CONCEPT.md
- Control flow tradeoffs: docs/bs/functional_goto.md
