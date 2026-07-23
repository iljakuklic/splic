# Kovács 2022 — *Staged Compilation with Two-Level Type Theory* (implementation-oriented rewrite)

Faithful summary of the implementation-relevant content, checked against the paper's
LaTeX source. Section references are to the paper.

## 1. The system (§2)

### 1.1 Universes
Universes `U_{i,j}` with stage `i ∈ {0,1}` and size `j ∈ ℕ`:
- `U0` — object-level (runtime) types. Every closed `A : U0` stages to a type of the
  object language.
- `U1` — meta-level (compile-time) types. Guaranteed to be computed away by staging.

Sizing (`j`) is orthogonal to staging. The surface syntax uses Russell-style universes;
the formal core uses **Coquand-style universes** (`U`, `El`, `Code` with `El ∘ Code = id`),
which elaboration inserts. There is no cumulativity; `NatElim` may eliminate from level `j`
into any level `k`.

### 1.2 Stage separation
Both universes may be closed under arbitrary type formers (Π, Σ, Id, inductives), but
**every type former, constructor and eliminator stays within one stage**:
- function domain and codomain are at the same stage;
- recursion/induction on `Nat0` can only target types in `U0`, meta recursion targets `U1`.

The two stages need not have the *same* type formers — see §5 below (variations).

### 1.3 The staging operations
None of these are expressible as functions (functions can't cross stages):
- **Lifting**: `A : U0` gives `⇑A : U1` — the type of metaprograms computing runtime
  expressions of type `A`.
- **Quoting**: `t : A : U0` gives `⟨t⟩ : ⇑A` — the metaprogram immediately returning `t`.
- **Splicing**: `t : ⇑A` gives `∼t : A` — during staging, the metaprogram runs and its
  result is inserted into the output. Splice binds tighter than application: `∼f x ≡ (∼f) x`.
- **Definitional inverses**: `∼⟨t⟩ = t` and `⟨∼t⟩ = t`. Formally, quote is an *invertible
  natural transformation* `Tm0 Γ A → Tm1 Γ (⇑A)`.

Note the stage index convention: `0` is runtime because a multi-level generalization would
lift `U_i` to `U_{i+1}` with a bottom-most object theory.

### 1.4 Contrast with MetaML / typed Template Haskell
Those systems have `Code A` where `A` and `Code A` live in the same universe of types, so
binder stages need extra scope-based disambiguation (e.g. TH top-level binders act as runtime
or static depending on quoting context). In 2LTT the stage of everything is enforced *purely
by typing* — no syntactic or scope-based restrictions. 2LTT is also the first system with
unrestricted staging *for types* (types computed by metaprograms), enabled by meta-level
dependent types + large elimination.

## 2. Programming patterns (§2.2–2.3)

- `id1 : (A : U1) → A → A` computes at compile time; applied to runtime data via
  `∼(id1 (⇑Bool0) ⟨true0⟩)`.
- `id⇑ : (A : ⇑U0) → ⇑∼A → ⇑∼A` — quantify over `⇑U0` (code of runtime types) when the
  type must be used at the object level, e.g. as a parameter of `List0 : U0 → U0`.
  There is no generic map from `U1` to `U0`, so `⇑U0`-quantification is the way to
  abstract over runtime types in meta code.
- Inlined map:
  `map : (A B : ⇑U0) → (⇑∼A → ⇑∼B) → ⇑(List0 ∼A) → ⇑(List0 ∼B)`
  implemented with `foldr0` under a quote; the meta function argument is inlined at
  each use site during staging.
- Compile-time recursion: `exp : Nat1 → ⇑Nat0 → ⇑Nat0` via `iter1`;
  `∼(exp 3 ⟨n⟩)` stages to `n *0 n *0 n *0 1`.
- **Staging types**: `Vec : Nat1 → ⇑U0 → ⇑U0` by meta-iteration produces nested pairs;
  `∼(Vec 3 ⟨Nat0⟩)` stages to `Nat0 × (Nat0 × (Nat0 × ⊤0))`. `map` over such vectors uses
  induction on `Nat1` and stages to fully unrolled projections.
- **Let-insertion** (ad hoc): bind a runtime expression with an object-level `let` and pass
  code of the *variable* into the metaprogram, so only the variable is duplicated.
- **Partially static data / partial evaluators**: a well-typed interpreter for an embedded
  language becomes a partial evaluator; e.g. `EvalTy : Ty → ⇑U0`,
  `EvalCon : Con → U1` (a *static* list storing *runtime* expressions),
  `EvalTm : Tm Γ A → EvalCon Γ → ⇑∼(EvalTy A)`; environment lookups are fully
  eliminated in the output.

## 3. Lifting properties, binding-time improvement, inference (§2.3)

### 3.1 Preservation of negative type formers
`⇑` has no computation rules but preserves negative type formers up to **definitional
isomorphism**:

```
⇑((x : A) → B x) ≃ ((x : ⇑A) → ⇑(B ∼x))     pres→ f := λ x. ⟨∼f ∼x⟩ ; pres→⁻¹ f := ⟨λ x. ∼(f ⟨x⟩)⟩
⇑((x : A) × B x) ≃ ((x : ⇑A) × ⇑(B ∼x))
⇑⊤0 ≃ ⊤1
```

Rewriting left-to-right is **binding-time improvement**: the improved form supports more
compile-time computation (meta λ instead of runtime λ). Going right-to-left introduces a
runtime binder — occasionally desirable to limit code size (like let-insertion).
The unimproved `id⇑ : (A : ⇑U0) → ⇑(∼A → ∼A)` stages to a useless β-redex
`(λ x. x) true0` — improved forms are the sensible default.

### 3.2 Positive types: serialization, cofibrancy, "the trick"
Inductive types are only preserved in one direction: `Bool1 → ⇑Bool0` exists
("serialization"); the other direction admits only constant functions (no elimination for
`⇑A` — code cannot be inspected). There is *no* map `(Nat1 → Nat1) → ⇑(Nat0 → Nat0)`.
But if `A : U1` is **finite** and `B` serializable, `A → B` is serializable (it's a finite
product) — `A` is called *cofibrant* in 2LTT jargon. This is the 2LTT form of the partial
evaluation "trick" (η-expanding functions out of finite sums). Fusion (foldr/build via
Böhm–Berarducci encoding, stream fusion via colists) is binding-time improvement for
general inductive types.

### 3.3 Inferring quotes and splices
Extract a **coercive subtyping** system used during bidirectional elaboration:
- `A ≤ ⇑A` (insert quote) and `⇑A ≤ A` (insert splice);
- contravariant–covariant rule for functions, covariant for Σ;
- optionally `U0 ≤ U1`, witnessed by `Lift` itself (a *type* coercion).

When comparing inferred vs. expected type, insert coercions. With this plus Agda-style
implicits + pattern unification, `map` can be written with no quotes/splices at all.
Where the elaborator must choose between improved/unimproved types, default to
**improved**, with explicit lifting to opt out. (See [`demo-implementation.md`](demo-implementation.md) for the
actual algorithm, including coercion avoidance.)

## 4. Staging: definition, algorithm, correctness (§3–5)

### 4.1 What staging is (Def. 3.1)
Staging maps 2LTT types/terms *in purely object-level contexts* to object-theory
types/terms:

```
Stage : Ty0,j ⌜Γ⌝ → TyO,j Γ        Stage : Tm0,j ⌜Γ⌝ A → TmO,j Γ (Stage A)
```

where `⌜–⌝` embeds object syntax into 2LTT. Properties:
- **Soundness**: `⌜Stage A⌝ = A` (up to conversion) — staging output is convertible to input.
- **Stability**: `Stage ⌜A⌝ = A` — staging is identity on splice-free terms.
- **Strictness**: the extracted algorithm preserves all type/term formers *strictly* —
  staging performs **no object-level β-reduction**. (Full normalization of 2LTT would be a
  sound+stable staging algorithm, but useless: no inlining control.)

Soundness + stability = embedding is a bijection up to conversion = **strong
conservativity** of 2LTT over the object theory.

Important distinction: strictness constrains the *staging output*, not definitional
equality. In this paper's 2LTT the object theory is full MLTT (Π with β/η, NatElim
β-rules), so *conversion checking during elaboration* does compute object-level redexes.
(Making object conversion weak is a separate design choice — see CFTT 2024.)

### 4.2 Staging-by-evaluation
Staging = evaluation of 2LTT syntax in the **presheaf model over the object theory's
syntactic category** (analogous to NbE). Intuition:
- Presheaves are "sets varying over object contexts"; the interpreter's semantic values may
  embed object-level types/terms that depend on their context.
- Meta-level types get standard semantic interpretations (`Nat1` ↦ ℕ, `Σ1` ↦ pairs,
  functions ↦ Kripke-style functions abstracting over context extensions).
- Object-level `Ty0`/`Tm0` are interpreted by *syntactic* object types/terms:
  `|A| : env → TyO Δ`, `|t| : (γ : env) → TmO Δ (|A| γ)`.
- `⇑A` is interpreted as `TmO[A]` — so quote and splice are **identity functions in the
  model**. Their cost is zero; all real work is meta-level computation.
- Open staging: interpret a purely-object context `Γ` by the **generic environment**
  `Γᴾ` = the list of `Γ`'s variables (≅ identity substitution). Stability falls out of this.

Everything in the model must be **stable under object substitution** (natural). This is the
core trade-off: metaprograms cannot observe things not preserved by substitution (e.g.
scope sizes, structure of code) — in exchange, implementations never need to track
object contexts/substitutions explicitly.

### 4.3 Extracted algorithm & optimizations (§3.4)
The naively extracted algorithm weakens semantic environments when going under object
binders (`γ[p]`) — potentially deep traversals. Standard fixes, used in the demo:
- **De Bruijn levels in the semantic domain** (weakening becomes free); indices in syntax.
- Closures in object-level binders; drop explicit substitutions from the core syntax.
- Untyped, tagged representation of semantic values (separate constructors for functions,
  literals, quoted expressions).
- Meta evaluation is closed (no free meta variables at staging time) and syntax-directed.
  **Meta-level types are erased during staging** (evaluated to a dummy value): they never
  appear in the output. Object-level types are staged for real, since they do appear in
  the output.
- **Caching/deduplication** of generated code (e.g. reusing specializations of `map` at the
  same arguments) is future work in the paper — a production system needs it.

### 4.4 Soundness proof shape (§5, skimmable)
Proof-relevant logical relation between the evaluation morphism and a *restriction*
morphism (2LTT syntax restricted to object contexts), defined internally to the presheaf
category. Only relevant if you want to port the correctness argument.

## 5. Variations of the object language (§2.5) — directly relevant to low-level targets

Restricting the object language makes it easier to compile; the meta level compensates.

### 5.1 Monomorphization
Object language is **simply typed** (every runtime type statically known — easy layout
and codegen):
- a judgment `A type0` for well-formed runtime types (closed under simple type formers);
- a meta-level type `Ty0 : U1` *replacing* `⇑U0`;
- for each `A type0`, `⇑A : Ty0`; quoting sends types `A type0` to `⟨A⟩ : Ty0` and terms
  `t : A` to `⟨t⟩ : ⇑A`.

2LTT still supports arbitrary higher-rank polymorphism over `Ty0` at compile time, e.g.
`((A : Ty0) → ⇑∼A → ⇑∼A) → ⇑Bool0` — it just must be staged away. The user-facing
restriction: **polymorphic functions cannot be stored inside runtime data**.

### 5.2 Memory-representation polymorphism
Refinement: internalize representations as a meta type and index runtime types by them:
- `Rep : U1` with e.g. `Ref : Rep`, `Prod : Rep → Rep → Rep`, primitive machine reps;
- `U_{0,j} : Rep → U_{0,j+1} r` — runtime universes indexed by representation;
- unboxed Σ: for `A : U0 r`, `B : A → U0 r'`, `(x : A) × B x : U0 (Prod r r')` —
  type dependency without representation dependency.

`Rep` is meta-level, so it cannot be abstracted over at runtime; staging computes all
`Rep` indices to canonical representations. This reconciles dependent types with memory
layout control. (Compare *Kinds Are Calling Conventions* for a much richer treatment of
the same axis — see [`downen-2020-kinds-are-calling-conventions.md`](downen-2020-kinds-are-calling-conventions.md).)

## 6. Intensional analysis (§6)

Analyzing the structure of `⇑A` values clashes with the standard semantics:
- Purely-object contexts are **representable** presheaves, so by the Yoneda lemma a
  meta-level function out of `⇑Bool0` has at most as many behaviors as `Bool` has
  elements — `decEq : (x y : ⇑Bool0) → (x =1 y) +1 (x ≠1 y)` cannot actually decide
  definitional equality. More directly: definitional *inequality* is not stable under
  substitution (unequal variables can be mapped to equal terms).

Two workable alternative setups, with trade-offs:
1. **Stability under weakenings only**: take only weakenings as base-category morphisms.
   Many analyses (including `decEq`, term strengthening/let-floating) are weakening-stable.
   Cost: without substitution in the object theory you cannot specify dependent or
   polymorphic object types (no dependent elimination / instantiation). Works fine for the
   monomorphization setup of §5.1.
2. **Closed modality**: a modality for *closed* object terms; closed terms are unaffected
   by substitution, so they can be analyzed (and safely `run`, MetaOCaml-style). C-like
   function pointers are a natural use case (closed after staging).

Default guidance stands: keep object code opaque — build (quote), compose (meta
functions), insert (splice) — unless you deliberately adopt one of the above setups.
The 2024 paper turns this opacity into a *feature* (generativity axiom).
