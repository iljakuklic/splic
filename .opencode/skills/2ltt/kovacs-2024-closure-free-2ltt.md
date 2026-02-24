# Kovács 2024 — *Closure-Free Functional Programming in a Two-Level Type Theory* (substantive rewrite)

This note rewrites the implementation-relevant parts of the 2024 system ("CFTT" in the paper).

## 1. Goal: staged compilation that eliminates abstraction overhead

The paper's theme:
- instead of relying on heavy optimizer passes, use staged metaprogramming to *compute away* abstraction layers (e.g. monads/transformers, stream fusion machinery),
- while producing object code that is well-typed and (in the presented object language) does not require dynamic closures.

---

## 2. Meta language vs object language

### 2.1 MetaTy (compile time)
MetaTy is a dependent type theory with:
- dependent functions, Σ-types
- indexed inductive types

This is where you implement libraries (monads, codegen, fusion) so they execute during unstaging.

### 2.2 Ty (object types) as a meta-level entity
The object type universe `Ty` lives as a meta type (`Ty : MetaTy`), i.e. object types are "data" at compile time but remain opaque in ways that preserve staged semantics.

---

## 3. Splitting object types: ValTy vs CompTy (closure-free core)

The object universe is split into sub-universes:

### 3.1 ValTy (value types)
- used for runtime-storable values
- supports algebraic data types (parameters may be general, but constructor fields are restricted to ValTy)
- `ValTy ⊆ Ty`

### 3.2 CompTy (computation types)
- used for computations that must not be stored as values
- contains (at least) function types, with restrictions that support closure-free execution
- `CompTy ⊆ Ty`

### 3.3 The key restriction that buys "no dynamic closures"
Functions (computations) cannot be:
- stored in data constructors
- passed as ordinary value arguments
- returned as values that escape scope

The runtime semantics can treat computations as call-by-name-ish in a controlled way, because they cannot be duplicated arbitrarily as first-class values.

Implementation note:
- the paper allows fairly liberal syntax (e.g. lambdas under case/let), but relies on a compilation step (call saturation / restructuring) to ensure calls become saturated in a way that avoids closures.

---

## 4. Object-level definitional equality is intentionally minimal

To keep typechecking and unstaging simple when object programs are embedded in a dependent meta system, object definitional equality is set up without:
- β/η rules for object functions
- let unfolding

Implementation takeaway:
- treat object terms as *code*, not as something you compute during typechecking.

---

## 5. Lift/quote/splice in CFTT

The bridge primitives are the same as in 2LTT:

- `⇑A : MetaTy` for `A : Ty`
- `⟨t⟩ : ⇑A` for `t : A`
- `∼u : A` for `u : ⇑A`

This yields *unstaging*: evaluate meta-level computation in splices and produce object code.

---

## 6. Binding-time improvements ("up/down") as library patterns

A recurring technique is to convert between:
- `⇑(A → B)` and `⇑A → ⇑B`
- and similarly for products, etc.

These conversions enable more compile-time computation by moving structure to the meta level.

Example pattern (functions):
- `up : ⇑(A → B) → ⇑A → ⇑B`
- `down : (⇑A → ⇑B) → ⇑(A → B)`

Engineering warning:
- these conversions can duplicate uses of code, so without care they can duplicate runtime computations.
- thus, you need let insertion / sharing (next section).

---

## 7. Let-insertion via a code generation monad (Gen)

To avoid duplication, the paper introduces a meta-level codegen facility (`Gen`) that generates object `let`/`letrec`.

Key operations (paraphrased into implementable intent):

- `gen : ⇑A -> Gen (⇑A)`
  - run the code `a`, bind it to an object-level `let x := ...`, and return code for `x`

- `genRec : (⇑A -> ⇑A) -> Gen (⇑A)` for computation types
  - generate object-level `letrec x := ...` for recursive computations

Running `Gen` yields code containing a sequence of lets with sharing.

Implementation note:
- a CPS representation of `Gen` is typical:
  it makes it easy to "emit lets" before continuing.

---

## 8. Monads & monad transformers: "Improve" class

The paper's library strategy:
- keep "real monads" at the meta level (so binds compute at unstaging time)
- convert object structures into meta monads and back when needed

This is packaged as a typeclass-like interface (conceptually):

- `up   : ⇑(F A) -> M (⇑A)`
- `down : M (⇑A) -> ⇑(F A)`

where:
- `F : ValTy -> Ty` is an object-level effect encoding
- `M : MetaTy -> MetaTy` is a meta-level monad (or monad transformer stack)
- using `up/down`, the meta-level monadic structure disappears during unstaging, leaving efficient object code

The paper works through examples like MaybeT/StateT/ReaderT in this style.

---

## 9. "No intensional analysis" + generativity (and why it matters)

A notable theme:
- the system benefits from the inability to inspect quoted object terms structurally.
- this "opacity" gives a parametricity-like payoff and supports an axiom ("generativity") that is validated in the staged semantics model used.

Implementation takeaway:
- if you implement `⇑A` as an opaque code type with only quote/splice/combinators, you naturally enforce the "no inspection" discipline.
- if you embed into a system like Template Haskell where you can inspect ASTs, the generativity principle fails; the paper discusses workarounds involving runtime-checked coercions that disappear if users respect the discipline.

---

## 10. Engineering summary: what to copy into your compiler/prototype

Minimum to reproduce the paper's practical benefits:
1. Two-stage calculus with `⇑/⟨⟩/∼`
2. Strong meta evaluation; weak object definitional equality
3. Let-insertion API (`Gen`) for sharing
4. Optional: closure-free object typing discipline (`ValTy`/`CompTy`)
5. Library patterns for binding-time improvements (`up/down`) and effect abstraction elimination
