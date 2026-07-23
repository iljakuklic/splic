# Downen/Ariola/Peyton Jones/Eisenberg 2020 — *Kinds Are Calling Conventions* (implementation-oriented rewrite)

An intermediate language (**IL**) where the *kind* of a type carries everything codegen
needs: representation, evaluation strategy, and function arity. Polymorphic code compiles
to a single block of machine code (type erasure, no monomorphization), with kind-level
side conditions ruling out exactly the uncompilable cases.

## 1. The three axes

- **Representation** — how a value is stored (heap pointer, machine int, …). Determines
  registers/moves.
- **Levity** — `L` (lifted: may be a thunk, evaluated lazily) vs `U` (unlifted: always a
  value, evaluated eagerly). Lets one IL serve both eager and lazy source languages.
- **Arity** — how many arguments (and of what representations) a primitive function
  needs before it does work; determines the call sequence.

Arity is *intensional* — types like `Int → Int → Int` don't determine it (`λx λy. e` has
arity 2; `λx. let z = expensive x in λy. e` has arity 1). The IL exists to *record*
the result of an arity analysis, not to perform it.

## 2. Kind grammar

```
κ ::= TYPE ρ ν                     -- every type former yields TYPE ρ ν
ρ ::= r | PtrR | IntR | ...        -- representation (r: rep variable)
γ ::= g | L | U                    -- levity (g: levity variable)
ν ::= n | Eval γ | Call[α]         -- convention (n: convention variable)
α ::= ρ, α | ε | arity(ν)          -- arity: list of argument reps
```

A type's convention is *either* `Eval γ` (data: levity) *or* `Call[α]` (primitive
function: arity) — functions are **called, not evaluated**, so they have no levity.
Examples:

```
Int# : TYPE IntR (Eval U)          IntL : TYPE PtrR (Eval L)
Int# ⤳ Int# ⤳ Int# : TYPE PtrR Call[IntR, IntR]
```

Reading the arrow kind: the representation `PtrR` is that of **the function value
itself** — a code/closure pointer — *not* of anything it takes or returns; the arity
`Call[IntR, IntR]` lists **only the argument** reps. The **return type's representation
never appears in the kind**: `Int# ⤳ Int#` and `Int# ⤳ Bool#` both have kind
`TYPE PtrR Call[IntR]`. That omission is deliberate — it is what lets return types stay
representation/levity-polymorphic (§4, tail-call return convention).

Haskell's default kind `★` = `TYPE PtrR (Eval L)`; an eager language's default is
`TYPE PtrR (Eval U)`. Function-type formation *concatenates* arities: if
`τ₁ : TYPE ρ₁ ν₁` and `τ₂ : TYPE ρ' Call[ρ₂,…,ρₘ]` then
`τ₁ ⤳ τ₂ : TYPE PtrR Call[ρ₁,ρ₂,…,ρₘ]` (if `τ₂` is `Eval γ`, arity is just `[ρ₁]`);
`arity(ν)` may be stuck on a convention variable. `∀` is kind-transparent (erased at
runtime) but its variable must not escape into the kind.

## 3. Explicit boxing, in two parallel instances

The same box/unbox pattern applies to representations and to arities; making both
explicit in IL is what lets the optimizer remove redundant round-trips:

| primitive (fast) | boxed (uniform) | box | unbox |
|---|---|---|---|
| `Int#` (machine int) | `Int γ` (heap) | `I# e` | `case e of I# x → …` |
| `τ ⤳ σ` (arity-n code) | `γ{τ ⤳ σ}` (closure) | `Clos e` | `App e` |

`Clos`/`App` convert between statically-called primitive functions and first-class
closures with a uniform (arity-1-ish) calling convention. Wherever a function must be
stored, passed at unknown convention, or kept as a value after erasure (e.g. CBV source
lambdas), it gets `Clos`-boxed.

## 4. Polymorphism restrictions: `mono-rep` / `mono-conv`

Instead of forbidding quantification over unboxed/function kinds (GHC's old "draconian"
rule — too restrictive, and broken by kind polymorphism), IL allows *all* quantification
and puts **side conditions on the term rules**:

```
Fun-I: λx:τ. e   requires  τ mono-rep
Fun-E: e e'      requires  τ mono-rep  and  τ mono-conv   (τ = argument type)
Clo-I: Clos e    requires the function's arity to be statically known
```

`mono-rep`/`mono-conv` = the representation/convention contains no variables. Rationale:
compiling an *application* requires knowing how the argument is stored (rep) and when to
evaluate it / what code shape to build for it (conv). A variant rule (`Fun-A-E`) relaxes
`mono-conv` when the argument is syntactically an answer (value) — lazy vs eager is then
indistinguishable. These conditions are validated by the lowering translation (§6): they
are exactly what the compilation scheme needs, no more.

What *can* be polymorphic — perhaps surprisingly much:
- `error : ∀ (r : Rep) (a : TYPE r ν). String → a` — never returns, so the result rep
  is irrelevant; one code block serves all instantiations.
- Return types may be rep/levity-polymorphic thanks to tail calls (the callee returns to
  the caller's caller): `revapp : ∀ n r g (t₁ : TYPE PtrR n) (t₂ : TYPE r (Eval g)). t₁ ⤳ (t₁ ⤳ t₂) ⤳ t₂`
  — `t₁` may be convention-polymorphic (only moved, never called/evaluated) but must be
  pointer-represented; `t₂` fully rep/levity-polymorphic but *not* conv-polymorphic
  (the λ-bound `f` gets called, so its arity must be known).
- `twice f x = f (f x)` must fix `Eval L` *or* `Eval U` for the intermediate result —
  polymorphism boundaries are exactly where a strategy decision is forced.
- Data types may be levity/convention-polymorphic:
  `data List (g : Lev) (n : Conv) (t : TYPE PtrR n)`; a fully strict function like `sum`
  is levity-polymorphic in everything, while `map` must pick the result spine's levity
  (it changes evaluation order — same IL definition, different machine code).

## 5. Equational theory: substitutability by type, not syntax

β for primitive functions fires only on **substitutable** arguments `S`, defined by
*kind*: all answers are substitutable; any expression of `Eval L` type is substitutable
(CBN-style); `Eval U` arguments must be reduced to answers first (CBV-style). This
integrates multiple evaluation orders in one calculus. Primitive function types enjoy
**unrestricted η** (in both directions) — precisely because functions cannot be observed,
only called. (Same property Kovács cites for CFTT's object language.)

## 6. Lowering to machine language (ML)

IL compiles (kind-directed, type-erasing) to **ML**: uncurried, fully η-expanded
functions, fully saturated calls, types = representations only. E.g. arity-3 `g` becomes
`λ(x:PtrR, y:PtrR, z:IntR). …` and every call site passes all three at once. The paper
also gives CBN and CBV System F translations *into* IL (CBN: everything `★ = TYPE PtrR
(Eval L)`; CBV: functions must be `Clos`-boxed to stay values), with correctness theorems
end-to-end. §7 adds optional *dynamic* arity dispatch on closures (runtime arity check to
use the best available calling convention).

## 7. Use with a 2LTT

KACC is orthogonal kit for the *object language*: index object types by
rep/levity/arity so unstaging emits codegen-determined code. In a 2LTT the meta level
replaces IL's quantifiers: rep/levity/arity polymorphism becomes meta-level abstraction
that staging eliminates, so the `mono-*` side conditions reappear as "these indices must
be canonical by staging time" (cf. the memory-representation-polymorphism variation in
[`kovacs-2022-staged-compilation-2ltt.md`](kovacs-2022-staged-compilation-2ltt.md) §5.2, and the CFTT ≈ simply-typed-IL-fragment
remark in [`kovacs-2024-closure-free-2ltt.md`](kovacs-2024-closure-free-2ltt.md) §7). A minimal adaptation: `Rep` as a meta
type, object types indexed by `Rep`, and — if functions are first-class — a `Clos`-style
boxing former to recover uniform representation where needed.
