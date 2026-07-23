# 2LTT Notes: Equality, Staging, and Coercions

Discussion notes on how definitional/propositional equality, conversion vs. staging,
weak object equality, and implicit coercions fit together in a two-level type theory,
and what that means for Splic specifically. Splic uses a **weak object equality** and is
deliberately spartan about implicit coercions; much of this document explains *why* those
are coherent choices and what constraints they impose.

Background reading: the `2ltt` skill (`.opencode/skills/2ltt/`), especially the Kovács
2022 (full-MLTT object theory) and 2024 (closure-free, weak equality) notes, and
[nbe_and_debruijn.md](nbe_and_debruijn.md) for the NbE machinery this all rests on.

## 1. Two equalities: definitional vs. propositional

Type theory has two distinct notions of equality, with different roles.

| | Definitional (judgmental) `≡` | Propositional `Id a b` |
|---|---|---|
| What it is | a **judgment** `Γ ⊢ a ≡ b : A`, part of typing | a **type**, inhabited by proof terms |
| Who applies it | the **typechecker**, silently | the **programmer**, explicitly |
| Decidable? | yes (that's the point) | no, in general |
| Contains | computation: β, η, ι, δ (unfolding), congruence, α | anything you can *prove* (induction, hypotheses) |
| You interact via | nothing — it just happens | `refl` to make, `J`/`transport` to use |

Slogan: **definitional equality is what the checker believes without being asked;
propositional equality is what you must prove and then explicitly transport along.**

### Where the checker uses definitional equality

Essentially one rule — conversion (change-of-type):

```
Γ ⊢ a : A     Γ ⊢ A ≡ B : U
───────────────────────────
        Γ ⊢ a : B
```

Every use of `≡` by the checker is an instance of this: it fires whenever an *inferred*
type must meet an *expected* type — application (`typeof arg ≡ domain`), checking mode
(`infer t` vs. the goal), unification (solving metas is deciding `≡` constraints), and, in
2LTT, deciding quote/splice coercions and stage adjustments.

### Where propositional equality enters

Only when the program text mentions it. It is data you construct and consume:

- **`refl`** is the bridge: checking `refl : Id a b` *calls* the conversion checker to
  decide `a ≡ b`. So `refl` typechecks iff the two sides are definitionally equal — the
  one automatic traffic between the two notions.
- **`J` / `transport`** is the explicit, term-level analogue of the conversion rule: where
  `≡` lets the checker retype silently, a propositional proof makes you write the coercion
  by hand.

### How they relate

1. **Definitional ⟹ propositional, canonically.** If `a ≡ b` then `refl : Id a b`.
   Definitional equalities are a *subset* of the provable ones.
2. **The converse fails, deliberately.** Having `p : Id a b` does *not* give `a ≡ b`.
   Adding that (equality reflection) yields Extensional Type Theory, where conversion
   becomes undecidable. Intensional Type Theory (which 2LTT and Splic are) keeps them
   apart so conversion stays mechanical.
3. So: put in `≡` only what a terminating normalizer can decide; everything deeper (needs
   induction, or is merely hypothetical) lives in `Id` and is carried explicitly.

## 2. Two definitional equalities in 2LTT, and the strength dial

In 2LTT the checker runs conversion at **both** stages:

- **`≡1`** (meta) — typechecks all meta code and drives staging decisions; includes meta β
  and the quote/splice inverses `∼⟨t⟩ ≡ t`, `⟨∼t⟩ ≡ t`.
- **`≡0`** (object) — invoked whenever object terms/types are compared. **Its strength is a
  design dial:**
  - **2022 (full MLTT object theory):** `≡0` has object β/η/ι. So
    `refl0 : Id0 ((λz.z) true0) true0` typechecks — the checker discharges it. But
    commutativity `Id0 (a+b) (b+a)` still needs an inductive `Id0` proof (it is not a
    computation rule).
  - **2024 CFTT / Splic (weak object theory):** `≡0` is α-equivalence only. Now even
    `(λz.z) true0 = true0` and `n + Z = n` fall *out* of `≡0` and become `Id0` proof
    obligations.

Restated: **weakening `≡0` moves facts from the "checker discharges it via `refl`" column
into the "you must prove it and `transport`" column.** You trade automation for
predictable staging (§5). `≡0` is exactly the set of code-shape differences the checker is
allowed to rewrite silently; in a compiler you often want that set empty.

## 3. Conversion vs. staging — different questions

These are separate operations that deliberately disagree, and it is not a contradiction.

- **Conversion** (elaboration time) asks: *may the checker interchange these two terms
  without changing well-typedness?* Governed by definitional equality.
- **Staging** (code-gen) asks: *what object code do I emit?* Governed by **strictness** —
  it performs **no object β**, preserving every object former exactly.

The sharp example: in the 2022 formulation, `⟨(λz.z) true0⟩ =1 ⟨true0⟩` **holds** as a
conversion fact, even though the redex sits *inside* the quote. The derivation is not a
meta reduction — it is object β lifted by congruence:

```
(λz.z) true0 ≡0 true0                (object β, part of object definitional equality)
────────────────────────────────    (congruence of the quote former ⟨_⟩)
⟨(λz.z) true0⟩ ≡1 ⟨true0⟩
```

Conversion looks *inside* object terms and reduces them; quote transports the resulting
equality up to the meta level. (Contrast `(λz.z) ⟨true0⟩`, which is a *meta* β-redex — a
meta function applied to code — an unrelated, trivial fact.)

But the same term, viewed by the two operations:

| | `⟨(λz.z) true0⟩` vs `⟨true0⟩` |
|---|---|
| **Conversion** (2022) | **equal** — object β fires inside the quote; `refl1` typechecks |
| **Staging** | **distinct outputs** — strictness forbids object β; emits `(λz.z) true0` and `true0` verbatim |

The payoff of the split: you can prove a metaprogram correct *reasoning as if the redex
reduced*, while still emitting the un-reduced code if that is what your staging logic
produced. Equational reasoning and the emitted artifact are decoupled — which is what lets
2LTT give both a usable equational theory and predictable codegen. **Staging is not
normalization**: full normalization would also be sound and stable, but it would β-reduce
object redexes and destroy inlining control.

### Under weak equality (CFTT / Splic)

With `≡0` weakened to α, that "free" conversion fact disappears:
`⟨(λz.z) true0⟩ ≠1 ⟨true0⟩`. Two quotes are equal iff their object syntax trees coincide
**up to bound-variable names (α), after all meta computation has run** — reflexivity,
symmetry, transitivity, congruence, and α, but *no* object β/η/let-unfolding/ι. It is not
the degenerate "each quote equal only to itself" (α-copies and meta-convertible quotes are
still equal); it is as syntactic as a sensible congruence can be.

Not equal under weak `≡0` (all would hold in full MLTT):

```
⟨(λz.z) t⟩         ≠1  ⟨t⟩            -- no object β
⟨n + Z⟩            ≠1  ⟨n⟩            -- no object computation
⟨let x = e in x⟩   ≠1  ⟨e⟩            -- no let-unfolding  ← the important one
```

## 4. In Splic: `val_eq` and stuck splices

Splic's definitional equality is `val_eq` (in `compiler/src/core/value.rs`): it `quote`s
both values to normal-form core terms and runs `alpha_eq`. So conversion = NbE-normalize +
α-compare. It is *weak* because `eval` computes very little:

- meta β and meta ι (match/`NatElim` at the meta level);
- quote/splice cancellation, both directions;
- **no object β** — the object language is first-order (object lambdas do not exist; `Lam`
  is meta-only), so on object code conversion is essentially structural.

**Stuck splices must be representable.** During NbE, a splice applied to something that is
not a concrete quote — a variable or unsolved meta — is stuck and must have *somewhere to
go*. In the reference domain this is a spine entry (`SSplice`) on a neutral; in Splic,
`eval` keeps a `Value::Splice` when its argument does not reduce to a `Value::Quote`.
Without such a representation, splices on neutrals are silently dropped (wrong output) or
crash. The dual cancellation `⟨∼n⟩ = n` on neutrals is equally required, so quote/splice
stay mutually inverse even on stuck terms — this is what keeps conversion complete.

## 5. How full MLTT `≡0` compromises control over generated code

A natural objection: if strictness insulates the output (staging never β-reduces), and
`≡0` "only runs in the typechecker," how does a strong object equality hurt control over
generated code? The narrow claim is correct — for a *fixed* elaborated core term, `≡0`
strength changes nothing emitted. But "used in the typechecker" ≠ "no effect on output,"
for two reasons.

1. **The checker inserts terms.** Elaboration synthesizes coercions, η-expansions, implicit
   arguments, and metavariable solutions to make a program typecheck; those inserted terms
   are staged *verbatim* (strictness preserves them). A richer `≡0` gives elaboration more
   license to insert — e.g. an unimproved lift stages to a *useless β-redex* `(λx.x) true0`,
   and coercive subtyping η-expands across object arrows. Strictness protects you from the
   checker *reducing* your code, not from it *inserting* code.
2. **Definitional equality erases distinctions.** `≡` is exactly the set of code-shape
   differences the type system treats as invisible. Full MLTT `≡0` makes
   `let x=e in b ≡ b[e/x]` (sharing), `(λz.z) t ≡ t` (unreduced redexes), and
   `λx. f x ≡ f` (arity/η-shape) — precisely the distinctions a low-level backend cares
   about. If the theory declares them equal, **no type or proposition can pin your output
   to a particular shape**; you cannot even *state* "this stays a shared `let`."

CFTT/Splic weaken `≡0` to α so those distinctions become **observable and enforceable**,
and the equalities you give up move into `Id0`, invoked deliberately by `transport`. Bonus:
object conversion becomes trivial to decide (α, not full normalization) — matching the
"object code is just data" stance.

## 6. What metaprograms may observe: no intensional analysis

Metaprograms build (quote), compose (meta functions), and insert (splice) object code, but
must **not inspect** it (no pattern-matching on `⇑A`). This is not a limitation of
convenience — inspecting code is *unsound* in the standard semantics.

### Why matching on code breaks: `decEq`

Suppose we allowed deciding equality of two code fragments and built:

```
f : (x y : ⇑Bool0) → ⇑Bool0
f x y = if decEq x y then ⟨true0⟩ else ⟨false0⟩
```

Work in object context `(a b : Bool0)`. Since `a`, `b` are distinct variables,
`decEq ⟨a⟩ ⟨b⟩` is false, so `f ⟨a⟩ ⟨b⟩` stages to `false0`. But staging must be **stable
under object substitution** (natural). Apply `σ = {a ↦ true0, b ↦ true0}`:

- stage then substitute: `false0` (closed literal — substitution does nothing);
- substitute then stage: both arguments become `⟨true0⟩`, so `decEq` is true and the result
  is `true0`.

`false0 ≠ true0` — staging is no longer natural; the output depends on the *syntactic
identity of variables*, which substitution may collapse. This is the concrete meaning of
"definitional *inequality* is not stable under substitution." The Yoneda view: a
purely-object context is a representable presheaf, so a meta function out of `⇑Bool0` has
at most `|Bool|` behaviors; `decEq` needs more, so there is no semantic value to interpret
it — it is simply not definable.

### What `decEq` would require

To write it you need a way to *look inside* code — an eliminator exposing the object AST:

```
data Code0 : U0 → U1 where
  CVar   : Name → Code0 A          -- ← the poison
  CTrue  : Code0 Bool0
  CFalse : Code0 Bool0
  CIf    : Code0 Bool0 → Code0 A → Code0 A → Code0 A
  …
reflect : ⇑A → Code0 A            -- the forbidden, undefinable map
```

The `CVar` case is the whole problem: it observes the identity of an object variable, which
substitution can destroy. The minimal forbidden primitive is just
`sameVar : ⇑Bool0 → ⇑Bool0 → Bool1`; that alone is non-natural. Note also this is only
*syntactic* equality — deciding *definitional* equality is strictly harder and fails the
same way. This is why real 2LTT gives `⇑A` no `data … where`.

## 7. Expressing propositions about behavior, not syntax

You often want the metalanguage to reason about the *behavior* of object code, not its
syntax. The reframing: in the standard model a value of `⇑A` already *is* an object term
**up to the object theory's definitional equality** — the metalanguage never saw raw
syntax; it sees the behavioral equivalence class. So you already have the behavioral view;
what differs is how much you can *prove* vs. *decide*.

- **Normalization is neither necessary nor sufficient.** A `normalize : ⇑A → ⇑A` that is
  definitionally the identity gives no new information; it becomes "useful" only if you then
  *inspect* the result, which is the non-natural step (a normal form can still contain a
  free variable — the `CVar` collapse applies unchanged). Normalizing-then-inspecting is
  exactly as broken as inspecting raw syntax.
- **State/prove ✅, decide ❌.** You can *state* behavioral propositions and *prove* them;
  you cannot *decide* them (get a `Bool1`/`Dec`) for open code — same Yoneda argument.
  Behavior being coarser than syntax does not help; deciding still requires detecting
  inequality.

The tools that actually give behavioral reasoning:

1. **Meta identity on lifts** for the *definitional* slice: `Id1 {⇑A} x y`. `refl1`
   typechecks iff the object terms are definitionally equal, and the conversion checker does
   the object normalization internally. Good for `⟨(λz.z) true0⟩ =1 ⟨true0⟩` (in the 2022
   object theory); useless for facts that need induction.
2. **Object propositional equality** `Id0` for everything deeper. A metaprogram can *build
   and compose* proof terms (`⇑(Id0 a b)` is just more code) — the sanctioned
   build/compose/insert, no inspection.
3. **Reify the object language as meta data** when you genuinely need to case-split on
   programs: define `Ty0`, `Tm Γ A` as *meta-level inductive types* with a denotation
   `⟦_⟧`. That data is closed, fully inspectable, substitution-stable, so you write your
   optimizer and prove `⟦opt t⟧ = ⟦t⟧` as an ordinary meta theorem, then compile back with
   an interpreter/quoter. This is the "well-typed interpreter *is* a partial evaluator"
   pattern — intensional analysis is fine there because you analyze your own AST, not
   primitive code.
4. **Closed modality** to *run* closed object terms (MetaOCaml-style) and observe actual
   runtime values.

### Worked example: proving `a + b = b + a`

The tempting statement `Id1 {⇑Nat0} ⟨a + b⟩ ⟨b + a⟩` (meta equality of *code*) is
**uninhabited** for open `a, b`: it needs the two sides definitionally equal, but with `+`
recursing on its first argument both are distinct stuck neutrals, and there is no
elimination on `⇑Nat0` to run an induction. Also `⇑(Id0 a b) ≠ Id1 ⟨a⟩ ⟨b⟩` — `Id` is a
positive type, not preserved by `⇑`.

The right target is the object propositional equality, proved by ordinary object-level
induction (all at stage 0):

```
comm : (n m : Nat0) → Id0 (n + m) (m + n)
comm Z0     m = sym0 (addZ m)                              -- addZ : n + Z = n
comm (S0 n) m = trans0 (cong0 S0 (comm n m)) (sym0 (addS m n))
                                                           -- addS : n + S m = S (n + m)
```

Then quote it up:

```
comm⇑ : (a b : ⇑Nat0) → ⇑(Id0 (∼a + ∼b) (∼b + ∼a))
comm⇑ a b = ⟨ comm ∼a ∼b ⟩
```

Division of labor: **proving** is object-level induction (the meta level cannot do it — no
elimination on `⇑Nat0` — and need not); **transporting** to compile time is a quote (free,
opaque); meta `Id1` on lifts only ever covers the definitional slice.

## 8. Implicit coercions in Splic

Splic inserts coercions as **explicit, typed core formers in check mode** — never as a
relaxation of `val_eq`. Current coercions: quote-insertion (checking a `Quote` against a
`Lift`), splice-insertion, and `Embed` (a meta int → `⟦object int⟧`). The general
fallthrough does *no* coercion — it infers and requires a hard `val_eq`, giving Splic its
spartan feel.

### The governing principle

An implicit coercion `coe : A → B` is safe to insert *silently* exactly when it is a
**definitional isomorphism**: there is an inverse with `coe⁻¹ ∘ coe ≡ id` and
`coe ∘ coe⁻¹ ≡ id`, where `≡` is *your* definitional equality. If the round-trip holds, the
coercion is invisible (inserting it changes no judgment; it can be erased). If not, it is an
**observable rewrite** that, under strict staging, changes emitted code.

So which coercions are allowed is decided by which round-trips `eval` can prove. In Splic
that is a sharp line:

- **`eval` proves**: meta β/ι, quote/splice cancellation.
- **`eval` does not prove**: any *object* β/η/let-unfolding.

Coercions needing only the first survive weak equality; coercions needing the second are
blocked.

### Catalog

| Coercion | Round-trip needs | Full-MLTT object eq | Weak object eq (Splic) |
|---|---|---|---|
| insert quote / splice, `A ↔ ⇑A` | quote/splice cancellation | ✅ silent | ✅ **silent** (in `eval`) |
| meta β/ι in a *type* | meta β/ι | ✅ | ✅ (meta computation kept) |
| BTI improvement iso `⇑(A→B) ↔ (⇑A→⇑B)` | object β **and** η | ✅ silent | ❌ observable rewrite |
| η-expand object fn to expected Π | object η | ✅ | ❌ (moot: no object fns) |
| surjective pairing on object Σ | object Σ-η | ✅ | ❌ |
| unfold an object `let` | object δ/ζ | ✅ | ❌ (sharing observable) |
| type-level *object* β `(λX.X) A ↝ A` | object β | ✅ (types equal) | ❌ distinct types |
| universe cumulativity `U0 ↝ U1` | a cumulativity rule | ✅ if declared | ❌ (no cumulativity) |
| meta int → object int (`Embed`) | *nothing* — one-way | ✅ | ✅ (see below) |

**Survives weak eq — quote/splice.** `t ↦ ⟨t⟩` with inverse `u ↦ ∼u`; round-trip is
`∼⟨t⟩ ≡ t` and `⟨∼u⟩ ≡ u`, both in `eval` regardless of object computation. This is the
deep reason 2LTT works over a weak object theory: the staging coercions rest on cancellation
laws, not on object β/η.

**Blocked under weak eq — the BTI improvement iso.** `f ↦ λx.⟨∼f ∼x⟩`, inverse
`g ↦ ⟨λx.∼(g ⟨x⟩)⟩`; composing back to `id` needs an object β and an object η. Under weak
eq the round-trip is not `id`, so it is an observable rewrite that inserts an object binder
and redex (the "useless β-redex"), emitted verbatim by strict staging. Do it as explicit
surface syntax (owning the generated code) or as a propositional `Id0` cast.

### The one exception: one-way meta-erased canonicalizers

`Embed` is not an iso — there is no object → meta projection — yet it is a valid silent
coercion because it is **total, deterministic, and computed away at staging**: it takes a
meta literal, evaluates it, and emits a canonical object literal. It needs no inverse
(the meta side is erased) and *cannot* have one. Constraints replacing "must be an iso":

- direction is meta → object only (compile-time collapses into canonical object code);
- it is a function of its input (deterministic — one output per input), so `val_eq` keeps
  unique normal forms;
- it fires only on inputs it can fully compute (here, meta literals).

### `Embed` is a serialization

`Embed : Int1 → ⇑Int0` is exactly a **serialization** in the skill's sense — the integer
instance of the `A1 → ⇑A0` pattern (cf. `Bool1 → ⇑Bool0`). It consumes a compile-time value
and emits object code of the corresponding literal. It is one-way because *deserialization*
`⇑Int0 → Int1` would require inspecting code (§6) — the serialization asymmetry, orthogonal
to weak equality (it holds even in a full-MLTT object 2LTT). Only the closed modality could
offer a restricted reverse ("run a closed piece of code").

### Definability as a conservativity sanity check

`Embed` is comfortable partly because it is *in principle definable* in the language: for a
finite type it is the case-split serialization `λn. ⟨case n of 0 → 0₀ | 1 → 1₀ | …⟩`, and
staging reduces both to `⟨lit_n⟩`. It is provided as a primitive only to skip the `2^w`
cases. The general principle:

> A primitive is sound to add iff it is **definitionally equal to a term you could already
> write** — a conservative extension. Anything you could observe or prove with it, you could
> already observe or prove without it.

This test doubles as an intensional-analysis alarm: deserialization feels wrong precisely
because it is *not* definable (no term of that type exists), so "can I write this as an
ordinary term?" catches both soundness leaks and forbidden observations. Two cautions:

- the equality must be *definitional*, not just same-typed — the primitive's `eval`/staging
  behavior must match the definition's, or it can smuggle in a new equation;
- definability is a *sufficient semantic* check; you still owe the operational wiring
  (deterministic `eval`/`quote`, a staging rule, meta-erased vs. object-emitted) so `val_eq`
  keeps unique normal forms.

### Practical checklist for adding a coercion

1. Is it a round-trip iso provable by `eval` alone (meta β/ι + quote/splice cancellation)?
   → safe silent coercion.
2. Is it a one-way, total, deterministic, meta-erased canonicalizer (Embed-shaped)? → safe.
3. Otherwise its justification leans on object β/η/let that Splic's equality lacks → do not
   insert it silently; make it explicit surface syntax, or a propositional `Id0` cast.
4. Give the new former a type (a `Prim` variant), an `eval`/`quote` rule (deterministic
   normal forms), and a staging rule; gate any general coercion behind a failed `val_eq` so
   real type errors are not masked; add tests that it fires exactly when intended.

The failure mode to watch for is a coercion that "obviously round-trips" under β/η reasoning
internalized from full MLTT — that reasoning is exactly what weak equality revokes.

## See Also

- `2ltt` skill — `.opencode/skills/2ltt/` (Kovács 2022 & 2024 notes, glossary, demo walkthrough)
- [nbe_and_debruijn.md](nbe_and_debruijn.md) — NbE, De Bruijn indices/levels, stuck-splice index shifting
- [pi_types.md](pi_types.md) — dependent function types and meta-level lambdas
- [self_typed_ir.md](self_typed_ir.md) — self-typed core IR
