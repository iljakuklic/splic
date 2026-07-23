# Kovács 2024 — *Closure-Free Functional Programming in a Two-Level Type Theory* (implementation-oriented rewrite)

Checked against the paper's LaTeX source. The system is called **CFTT** ("closure-free
type theory"). This file only covers what CFTT *adds or changes* relative to the 2022
2LTT — for the shared core (lift/quote/splice rules, staging-by-evaluation,
soundness/stability/strictness, binding-time improvement, inference) see
[`kovacs-2022-staged-compilation-2ltt.md`](kovacs-2022-staged-compilation-2ltt.md).

Terminology: this paper says **unstaging** for what the 2022 paper calls "staging"
(running metaprograms in splices to extract object code). Same thing.

## 1. What changes vs. 2022

| | 2022 2LTT | 2024 CFTT |
|---|---|---|
| Object language | full dependent MLTT | **simply-typed, first-order**, general recursion, finitary ADTs |
| Object definitional equality | full β/η | **none** (no β, no η, no let-unfolding) |
| Object universe | `U0`, a stage-0 universe | `Ty : MetaTy` — object types are meta-level *data*, split into `ValTy`/`CompTy` |
| Meta language | MLTT | MLTT + indexed inductive families (via W-types) + identity type |
| Closures at runtime | possible | **guaranteed absent** |

Goal: shift work from general-purpose optimizers to metaprograms; get *guarantees* that
abstraction (monads, transformers, fusion) is eliminated at staging time.

## 2. The object language

### 2.1 ValTy / CompTy
`Ty : MetaTy`, with two sub-universes (formally Tarski-style with explicit inclusions
`V : ValTy → Ty`, `C : CompTy → Ty`; surface syntax uses implicit coercion):

- **`ValTy`** — value types: runtime-storable, call-by-value. Supports parameterized ADTs;
  parameters may have arbitrary types but **all constructor fields must be in `ValTy`**.
- **`CompTy`** — computation types: call-by-name, *not* storable. Contains:
  - functions `_→_ : ValTy → Ty → CompTy` — domain must be a value type, codomain
    arbitrary (so `Bool → Bool → Bool` is fine, `(Bool → Bool) → Bool` is ill-formed);
  - **finite products of computations**: `() : CompTy`, `(_,_) : CompTy → CompTy → CompTy`
    with pairing/projections. These exist to express **mutually recursive** function
    blocks: a `letrec` at type `(A → B, A → B)` compiles to a pair of mutual functions.

Since object types are meta-level terms, **object types never depend on object terms**
(no runtime type dependency) — a key invariant used by generativity (§6).

### 2.2 Binders and recursion
- `let x : A := t; u` — non-recursive, any type, allows shadowing. Object-level
  definitions use `:=`, meta-level use `=` (stages of definitions are always explicit).
- `letrec x : A := t; u` — **computations only** (only functions/computation products can
  be recursive). General recursion, no termination checking.
- λ-abstractions are allowed under `case` branches and `let` bodies (liberal syntax chosen
  deliberately: unrestricted `let`-insertion into any position is what makes
  metaprogramming convenient).

### 2.3 Why no closures are needed
Computations cannot be stored in constructors, passed as (value) arguments, or escape
their scope. With call-by-name semantics for computations, every program can be
transformed so that **every call is saturated** (`f t1 … tn` where `f`'s definition
immediately λ-binds `n` args) — e.g. `case b of True → λx.x+10; False → λx.x*10`
transforms to `λb x. case b of …`. Local functions then compile to lambda-lifted
top-level functions or join points. The call-saturation translation is formalized (per
step) in the paper's Agda supplement; CBN for computations causes no significant work
duplication *because* computations can't be duplicated as first-class values.

Contrast CBPV: similar value/computation split, but CBPV only binds value variables
(functions must be thunked into values to be let-bound) — unusable here. Contrast
defunctionalization: that makes closures *transparent* (constructor + dispatch) but does
not remove dynamic control flow; unstaging instead runs higher-order metaprograms that
never put functions in the output.

### 2.4 Object definitional equality: none
No β, no η, no let-unfolding for object programs, because:
- code size/efficiency are the point of staging, and they are **not stable under
  βη-conversion or let-unfolding**;
- with general recursion there is no decidable, sensible program equivalence anyway.

Typechecking compares object code essentially syntactically (this is the "weak object
equality" option; the 2022-style full-MLTT object theory is the other end of the
spectrum). Consequence: `up`/`down` (below) cannot be proven inverse internally — that's
expected and fine.

## 3. The code generation monad `Gen`

Binding-time improvement `up : ⇑(A,B) → (⇑A, ⇑B)` duplicates its argument — code
duplication and repeated runtime work. Let-insertion is impossible in a plain meta
function (can't introduce object binders from `MetaTy`). Fix: CPS code generators
(Bondorf-style), packaged as a monad:

```
newtype Gen (A : MetaTy) = Gen { unGen : {R : Ty} → (A → ⇑R) → ⇑R }

return a  = Gen λk. k a
ga >>= f  = Gen λk. unGen ga (λa. unGen (f a) k)

runGen : Gen (⇑A) → ⇑A          runGen ma = unGen ma id

gen    : {A : Ty}    → ⇑A → Gen (⇑A)          -- let-insertion
gen a    = Gen λk. ⟨let x : A := ∼a; ∼(k ⟨x⟩)⟩

genRec : {A : CompTy} → (⇑A → ⇑A) → Gen (⇑A)  -- letrec-insertion
genRec f = Gen λk. ⟨letrec x : A := ∼(f ⟨x⟩); ∼(k ⟨x⟩)⟩
```

The **answer type `R` is polymorphic** (a deliberate improvement over prior art with a
fixed answer-type parameter): generators need not anticipate the output type.
`gen` returns code that is a *variable*, so reuse is free. Don't write everything in
`Gen` reflexively — implicit emission makes generated-code size harder to reason about
(analogy: `IO` in Haskell).

## 4. The monad-transformer library pattern

Strategy: keep *real* monads at the meta level (their binds compute at unstaging time);
convert to/from object-level effect encodings only at runtime boundaries.

```
class Monad M => MonadGen M where liftGen : Gen A → M A
  -- gen/genRec generalize to any MonadGen

class MonadGen M => Improve (F : ValTy → Ty) (M : MetaTy → MetaTy) where
  up   : {A : ValTy} → ⇑(F A) → M (⇑A)     -- object action → meta action
  down : {A : ValTy} → M (⇑A) → ⇑(F A)     -- meta action → object code
```

- Base case: `Improve Identity Gen`.
- Compositional: `Improve F M => Improve (MaybeT F) (MaybeTₘ M)`, similarly
  `StateT S` (with `S : ValTy`, improved as `StateTₘ (⇑S) M`) and `ReaderT`.
  Meta side reuses standard `mtl` definitions unchanged. `ContT` is the one transformer
  that cannot be improved (needs real closures).
- Object-level recursive calls inside monadic code: just wrap in `up ⟨f ∼x⟩`.
- `modify`/`put`/`local` naively substitute *expressions* for the state (inline-style,
  duplicating work); define strict variants that `gen`-bind first, e.g.
  `put' s = do {s ← gen s; put s; return ⟨()⟩}`.

### 4.1 Case splitting on object values ("the trick", monadic form)
Cannot eliminate `⇑A` into `MetaTy` directly; instead *generate* an object `case` whose
branches continue code generation:

```
data SplitList A = Nil' | Cons' (⇑A) (⇑(List A))
split : ⇑(List A) → Gen (SplitList A)
split as = Gen λk. ⟨case ∼as of Nil → ∼(k Nil'); Cons a as → ∼(k (Cons' ⟨a⟩ ⟨as⟩))⟩
```

Generalized as a class `Split (A : ValTy) { SplitTo : MetaTy; split : ⇑A → Gen SplitTo }`.
A native implementation would elaborate `case` on object values in do-notation to `split`.

### 4.2 Join points (`MonadJoin`) — avoiding exponential code size
Monadic bind after a `split` continues generation *in each branch*: sequential Boolean
cases ⇒ exponential blowup. Naive fix (`gen` the branchy action via `down`/`up`) forces
runtime constructors + re-matching. Better: **join points** — let-bind one continuation
per constructor of the result, fusing away the constructors:

- `USOP = List (List ValTy)` — a Tarski universe of **sums of products** of value types,
  with decoding `El_SOP : USOP → MetaTy`; closed under value types, finite sums, finite
  products (products taken at the *meta* level, so they keep β/η — this is why SOP, not
  plain finite sums).
- `class IsSOP (A : MetaTy) { Rep : USOP; rep : A ≃ El_SOP Rep }` (isomorphism with
  proofs). Conjectured to coincide with the *cofibrant* types of 2LTT.
- `Fun_SOP : USOP → Ty → CompTy` tabulates `El_SOP A → ⇑R` as a computation product of
  first-order functions; `tabulate`/`index` convert back and forth; `letrec` over it
  yields mutually recursive functions.
- `class Monad M => MonadJoin M { join : IsSOP A => M A → M A }`; the `Gen` instance
  tabulates the continuation, `gen`-binds every join point, then dispatches. Transformer
  instances just delegate (StateT additionally needs `IsSOP S`).

Rule of thumb: `join` every object-level case with ≥2 branches (dead join points are
trivial downstream cleanup); skip it when a branch short-circuits at the meta level
(e.g. `fail` in `MaybeT`).

## 5. Pull-stream fusion (dependent types earning their keep)

```
data Step S A = Stop | Skip S | Yield A S
data Pull A where
  Pull : (S : MetaTy) → IsSOP S ⇒ Gen S → (S → Gen (Step S A)) → Pull A
```

Machine states are meta-level SOP data; `foldr` tabulates the transition function into a
`letrec`-bound computation product of **mutually recursive** functions (one per state
shape), and e.g. `foldl` derived from `foldr` produces tail-recursive accumulator code
with no closures. Combinators (`zip`-style applicative, `<>`, `filter`, `take`…) are the
standard Coutts-style definitions plus `Gen`/`IsSOP` noise.

`concatMap : IsSOP A ⇒ (A → Pull B) → Pull A → Pull B` needs the inner machine state
`Σ A (projS ∘ f)` to be SOP — i.e. **`USOP` closed under Σ** — which needs generativity:

## 6. Generativity: exploiting the *absence* of intensional analysis

**Axiom.** For `f : El_P A → USOP` (domain: finite products of object terms; codomain: a
"constant" metatype) and any `x y`, `f x = f y` — such functions are constant. Justified
in the presheaf model by Yoneda: products of object terms form a representable presheaf,
`USOP` a constant one, so natural transformations between them are constant. It reflects
that metaprograms cannot inspect object *terms* (inspecting object *types* would be
consistent, though).

Used to define `Σ_SOP`: instantiate the family at arbitrary inhabitants (products of
`letrec x := x` loops) — generativity says the choice doesn't matter — then concatenate
`A_i × B(inject_i loop)` per summand. With β/η for meta products, this gives
pairing/projections and the `IsSOP (Σ A B)` instance.

**Soundness up to erasure**: axioms block computation, so unstaging first *erases all
identity proofs and transports* (a syntactic translation into CFTT + equality
reflection), then evaluates. Soundness of unstaging holds up to this erasure; strictness
is trivial (no object conversion rules); stability as in 2022. Implementation notes: the
Agda embedding uses `primTrustMe` to erase the axiom; in typed Template Haskell
generativity is *false* (quotes can be inspected), so runtime-checked coercions are used
which vanish if users respect the discipline.

## 7. Practical notes

- Quote/splice/`up`/`down` noise is expected to be almost fully inferable with
  bidirectional elaboration + the subtyping of the 2022 demo, *given* explicit stages on
  let-definitions.
- Closure-freedom costs surprisingly little; a real language would add an opt-in closure
  type former (`CompTy → ValTy` boxing) and keep both, plus push streams (a proper
  monad, bottom of transformer stacks) alongside pull streams (top of stacks).
- Downstream compiler still wants: dead code elimination, unused-arg removal,
  de-duplication of generated code.
- The object language is close to a simply-typed fragment of the KACC intermediate
  language (see [`downen-2020-kinds-are-calling-conventions.md`](downen-2020-kinds-are-calling-conventions.md)): function types distinct
  from closure types, universal η, explicit arity — KACC lacks only `letrec`.
