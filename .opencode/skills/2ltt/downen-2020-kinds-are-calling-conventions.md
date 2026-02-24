# Downen/Ariola/Peyton Jones/Eisenberg 2020 — *Kinds Are Calling Conventions* (substantive rewrite, implementation-focused)

This note extracts and rewrites the parts most relevant to implementing a staged language where types carry low-level compilation information.

## 1. Motivation: polymorphism vs efficient code

Polymorphism is great for source languages, but compilers need concrete calling conventions and concrete representations to generate efficient code.

This work proposes an intermediate language (IL) where you can still write polymorphic programs, but the type/kind system tracks:
- runtime representation of values (boxed pointer vs unboxed int, etc.)
- arity / calling convention information for functions
- evaluation order / strictness (e.g. call-by-name vs call-by-value variants)

Key slogan:
- store calling-convention info in **kinds** (or kind-like indices), not as ad-hoc compiler metadata.

## 2. "Boxing/unboxing is explicit" as an optimizer-enabling design

A recurring pattern in the IL:
- There is a primitive/unboxed representation (fast, low-level)
- There is a boxed/wrapped representation (uniform, convenient)
- There are explicit constructors/destructors to move between them

Examples discussed include:
- closure wrappers for arity/evaluation control (`Clos`/`App`)
- boxed vs unboxed integers (`I#` and case on `I#`)

Implementation takeaway for a staged system:
- if metaprograms can choose representations, you want those choices reflected explicitly in the object language output so downstream optimization is simpler and more reliable.

## 3. Indexing types by representation and convention (core technique)

A simplified view of the IL's approach:

### 3.1 Representation indices
Introduce a kind/index `Rep` describing runtime storage (pointer, int register, etc.).

Then define a family `TYPE : Rep -> *` (or "types classified by representation").
So a type is not just `Int`, but `Int : TYPE IntR` (illustrative).

### 3.2 Conventions / levity / evaluation order indices
In addition to representation, the paper supports indices that describe:
- evaluation strategy of arguments/results
- function arity / calling protocol
- (and related "levity polymorphism" concerns)

The point is to precisely control what can be polymorphic:
- some functions can be representation-polymorphic
- but some combinations are rejected because codegen would not know how to pass arguments

The paper gives examples where a seemingly innocent "polymorphic application helper" must be rejected unless it restricts representations to pointer-like ones, because otherwise the call sequence is not statically determined.

## 4. Type system structure (what to reuse)

The IL has typing rules that:
- prevent "unknown representation" values from being used in ways that require a concrete calling convention
- ensure you only call primitive ops with the right number/kind of arguments
- ensure closure wrappers are used where needed to preserve language-level semantics (e.g. call-by-value polymorphic lambdas must remain values after erasure)

Implementation takeaway for 2LTT/CFTT:
- if you want *layout control* as a staged feature, adopt the same discipline:
  make representation/convention indices part of object typing, so unstaging outputs code that is already "codegen-determined".

## 5. How this complements 2LTT

2LTT gives you:
- a meta language where you can compute programs/types

KACC gives you:
- a way to make the object language's types rich enough to express low-level calling/representation choices safely

Combined design sketch:
- Meta level computes object types that include representation indices.
- Unstaging produces an IL-like object program whose typing guarantees calling convention correctness.

This is especially relevant if you want to reproduce "memory layout control" / "monomorphization-by-staging" style applications mentioned in the staging literature.

## 6. Practical "minimum viable" adaptation

If the full IL is too large, start with:
- `Rep = Ptr | I64 | F64 | ...`
- `Ty rep` object types, so every object term is typed with a rep
- restrict polymorphism so that:
  - fully representation-polymorphic functions can only do things that are representation-agnostic
  - calling a function at a rep-polymorphic type is restricted unless you wrap it into a uniform calling convention (closure) that erases the rep differences

This gives you a stepping stone toward the richer kind discipline described in the paper.
