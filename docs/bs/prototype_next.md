# Prototype: Next Steps

This document outlines logical next steps after the basic prototype is complete.

## 1. Meta-level Functions

Add support for first-class meta-level functions (functions that operate on code at compile time).

### Repeated Application Example

```splic
fn repeat(f: [[u64]] -> [[u64]], n: u64, x: [[u64]]) -> [[u64]] {
    match n {
        0 => x,
        n => repeat(f, n - 1, #(f(x))),
    }
}

code fn square_twce(x: u64) -> u64 {
    $(repeat(|y| #($(y) * $(y)), #(x)))
}
```

Expands to:
```splic
code fn square_twce(x: u64) -> u64 {
    (x * x) * (x * x)
}
```

This requires:
- Meta-level function types: `[[A]] -> [[B]]`
- Function application at meta level

## 2. Product Types

Add tuples or user-defined structs.

### Option A: Tuples

```splic
let p: (u64, u8) = (42, 8);
let x = p.0;
let y = p.1;
```

### Option B: Structs

```splic
struct Point { x: u64, y: u64 }
let p = Point { x: 42, y: 8 };
let x = p.x;
```

Decision deferred—see [tuples_and_inference.md](tuples_and_inference.md).

## 3. Dependent Function Types at Meta Level

The Vec3 example from Kovács 2022 demonstrates staged type generation:

### Vec3 Type (Compile-time Sized Vector)

```splic
fn Vec(n: u64, A: [[VmType]]) -> [[VmType]] {
    match n {
        0 => #(u0),
        n => #((A, $(Vec(n - 1, A)))),
    }
}

fn Tuple3(A: [[VmType]]) -> [[VmType]] { Vec(3, A) }
// Tuple3(#(u64)) → #((u64, (u64, (u64, 0_u0))))
```

After staging, `Tuple3(#(u64))` normalizes to a concrete product type.

### Staged Map (Following Vec Definition)

A `map` function that is defined in terms of Vec—the recursion happens at compile time, unrolling the map for the given size:

```splic
fn map(n: u64, f: [[u64]] -> [[u64]], xs: [[Vec(n, u64)]]) -> [[Vec(n, u64)]] {
    match n {
        0 => #(0_u0),
        n => #(
            let (x0, xs0) = $(xs);
            let x0_new = $(f #(x0));
            (x0_new, $(map(n - 1, f, xs0)))
        ),
    }
}

code fn example(x: $(Vec(3, u64))) -> $(Vec(3, u64)) {
    $(map(3, #(y => y + 2), #(x)))
}
```

Expands to:
```splic
code fn example(xs: (u64, (u64, (u64, u0)))) -> (u64, (u64, (u64, u0))) {
    let (x0, xs0) = xs;
    let x0_new = x0 + 2;
    let (x1, xs1) = xs0;
    let x1_new = x1 + 2;
    let (x2, xs2) = xs1;
    let x2_new = x2 + 2;
    (x0_new, x1_new, x2_new, 0_u0)
}
```

The key point: the `map` function recursively generates code at compile time based on the size `n`.

### Benefits

- Compile-time code generation via meta-level recursion
- Staged types that depend on compile-time values (Nat1 in original 2LTT)
- Object-level code with no runtime overhead from staging

## References

- Kovács 2022: Staged Compilation with Two-Level Type Theory (ICFP)
- Vec / map examples: [kovacs-2022-icfp22-slides.md](../../.opencode/skills/2ltt/kovacs-2022-icfp22-slides.md)
