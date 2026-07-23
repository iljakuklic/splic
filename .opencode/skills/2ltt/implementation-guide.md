# Practical implementation guide for 2LTT-based staged compilers

The "do this in code" companion to the paper notes in this skill. Theory lives in the
paper files; this file covers architecture, design decisions, and pitfalls. Notation:
`⇑A` lift, `⟨t⟩` quote, `∼t` splice, `U0`/`U1` object/meta universes.

## 1. Pipeline shape

```
parse → elaborate (bidirectional, NbE conversion, metavariables) → zonk → stage → backend
```

Two distinct evaluation mechanisms, easy to conflate:

1. **Conversion-checking evaluation** (inside elaboration): decides definitional
   equality of types/terms. Operates on one mixed semantic domain. Whether it computes
   *object*-level redexes is a design decision (§2.1).
2. **Staging** (after elaboration): runs all metaprograms, eliminates every
   quote/splice, outputs pure object syntax. Never β-reduces object code
   (*strictness*), regardless of what conversion does.

Both are NbE-style: eval into a semantic domain with De Bruijn levels + closures, read
back into syntax with indices. See [`demo-implementation.md`](demo-implementation.md) for the reference shapes
(§2 conversion evaluator, §3 two-domain staging evaluator).

## 2. Design decisions to make up front

### 2.1 Object language strength ⇒ object definitional equality
Two coherent points in the space — pick one deliberately:

- **2022-style**: object level is a full dependent type theory with β/η. Conversion
  checking must then compute object redexes too (the demo evaluator is stage-agnostic).
  Needed if object types depend on object terms.
- **CFTT-style** (2024): object level is simply-typed/first-order with general
  recursion; **no β, no η, no let-unfolding** for object code — conversion compares
  object terms essentially syntactically. Rationale: code size/efficiency aren't stable
  under βη; general recursion has no decidable equality anyway. Object types are
  meta-level data (`Ty : MetaTy`) and never depend on object terms.

Mixing them accidentally (e.g. weak equality but object-term-dependent types) breaks
things: dependent typing needs substitution and conversion at the object level.

### 2.2 Stage bookkeeping: repair vs. reject
Record the stage of **every binding** in the elaboration context, and have `infer`
return `(Term, VTy, Stage)`. On a stage mismatch there are two designs:

- **Repair (reference demo)**: coercive subtyping `A ≤ ⇑A` (insert quote), `⇑A ≤ A`
  (insert splice), optionally `U0 ≤ U1` (insert `Lift`), with contravariant/covariant
  function rule. Powerful inference (quotes/splices mostly disappear from surface
  syntax); costs a coercion pass, explicit weakening in the core, coercion-avoidance
  logic. See [`demo-implementation.md`](demo-implementation.md) §4.2–4.3.
- **Reject**: hard "stage mismatch" error at the point of use. Much simpler; forces
  explicit staging operators in the surface language. Fine as a first iteration —
  the type structure is identical, only elaboration ergonomics differ.

Either way: **explicit stages on `let`-definitions** (no stage metavariables). The
reference implementation found this single annotation makes the rest of stage inference
effective and stage unification unnecessary.

### 2.3 Optional extensions (see the respective files)
- Closure-free discipline `ValTy`/`CompTy`, computation products, call saturation —
  [`kovacs-2024-closure-free-2ltt.md`](kovacs-2024-closure-free-2ltt.md) §2.
- Representation/arity/levity indexing of object types —
  [`downen-2020-kinds-are-calling-conventions.md`](downen-2020-kinds-are-calling-conventions.md); meta-level `Rep` indexing —
  [`kovacs-2022-staged-compilation-2ltt.md`](kovacs-2022-staged-compilation-2ltt.md) §5.2.
- Intensional analysis (needs a non-standard setup) —
  [`kovacs-2022-staged-compilation-2ltt.md`](kovacs-2022-staged-compilation-2ltt.md) §6.

## 3. NbE core (checker)

Terms use **De Bruijn indices** (0 = nearest binder); semantic values use **levels**
(0 = outermost), so weakening of values is free and staging/splicing needs no shifting.

```
lvl_to_ix(depth, lvl) = depth - lvl - 1     (and symmetrically ix→lvl)
```

```rust
// shape, not literal code
enum Value {
  Rigid(Lvl, Spine),          // stuck on a variable
  Flex(MetaVar, Spine),       // stuck on an unsolved meta (if you have metas)
  Lam(Name, Closure),
  Pi(Name, VTy, Closure),
  U(Stage), Lift(VTy), Quote(Value),
  ...
}
```

- Closures: `{ env, body }` (or a host-language function). Substitution is *never*
  performed on syntax; going under a binder extends the environment with a fresh
  `Rigid(depth)`.
- Spines record stuck eliminations. **A stuck splice is a spine entry** (or a dedicated
  neutral): `splice(Rigid x sp) = Rigid x (sp . Splice)`. Cancellation both ways:
  `∼⟨t⟩ = t` in `vSplice`, and `⟨n·∼⟩ = n` when quoting a neutral ending in a splice.
  Without a stuck-splice representation, `eval` on a splice of a variable has nowhere
  to go and either panics or drops the splice.
- Read-back (`quote : Lvl → Value → Term`, a.k.a. "quotation" in NbE jargon — distinct
  from the staging operation ⟨⟩!) applies closures to fresh rigids at the current depth.

## 4. Elaboration guardrails

These invariants prevent whole classes of bugs:

- **`infer` returns `(Term, VTy)`** (plus stage). Never recover a type after the fact by
  pattern-matching elaborated terms — a `typeOf`-style helper is a red flag that
  `infer` returns too little.
- **`checkU`**: when a term must be a type, check against the universe directly —
  `checkU cxt t s = check cxt t (VU s) s`. Don't infer-then-test with an
  `isUniverseType` predicate.
- **Quote insertion in `check`**: when checking any non-quote term against `⇑A`, check
  the term against `A` at stage 0 and wrap with quote. Sound because every value of
  `⇑A` is `⟨t⟩` up to definitional equality; big ergonomics win.
- **Smart constructors** `tQuote`/`tSplice` that cancel `Quote(Splice t)`/`Splice(Quote t)`
  syntactically, so elaboration output stays small and staging sees fewer no-ops.
- If using metavariables: **zonk before staging**; staging must treat a remaining
  unsolved meta as a hard error with a good message.

## 5. Staging pass essentials

(Reference shape: [`demo-implementation.md`](demo-implementation.md) §3.)

- Separate pass over elaborated (zonked) syntax; do not reuse the conversion evaluator.
- **Two value domains**: meta values (functions-as-closures, inductive values, quoted
  object values) and object values (mirror of object syntax with levels + closures).
  One environment with stage-tagged entries.
- `eval_meta` β-reduces and runs eliminators; **erases all meta-level types to a dummy**
  (they cannot appear in output). `eval_obj` is structural — it only resolves variables
  and splices; `Splice t → eval_meta t` must yield a quoted object value, anything else
  is a compiler bug (`unreachable!`, not a user error).
- Output invariant: read-back of the object value contains no quote, splice, lift, or
  meta-level residue. Worth asserting in tests.
- Correctness properties worth testing: *stability* (staging splice-free input is
  identity) and *strictness* (object redexes in the input survive verbatim).

## 6. Code generation quality

Meta-level use of object code duplicates it (using `x : ⇑A` twice pastes the expression
twice). The toolkit — `Gen` monad (CPS let-insertion, polymorphic answer type),
`gen`/`genRec`, case-splitting via `Split`, join points via `MonadJoin` + SOP — is
library-level metaprogramming, specified in [`kovacs-2024-closure-free-2ltt.md`](kovacs-2024-closure-free-2ltt.md) §3–4.
A staged compiler doesn't need built-in support, but its object language must offer
`let`/`letrec` insertable at arbitrary positions for these libraries to be writable.
Deduplication/caching of generated code across splice sites is an open engineering
problem (flagged in both papers).

## 7. Pitfalls checklist

- Dropped stuck splices (missing neutral case) — silent wrong output. (§3)
- Object β performed during staging — violates strictness, destroys inlining control.
- Conversion checker computing object redexes in a CFTT-style design (or failing to in
  a 2022-style design). (§2.1)
- Quote/splice cancellation implemented in only one direction.
- Meta code leaking into staging output because meta types weren't erased or a meta
  binding was staged as object.
- Recovering types from elaborated terms instead of returning them from `infer`. (§4)
- Index/level mix-ups when splicing object values into deeper contexts — use levels in
  all semantic domains; convert only at read-back.
- Treating internal invariant violations after type checking as recoverable errors —
  they are bugs; fail loudly.

## 8. Reference implementations

- **elaboration-zoo** (Kovács): https://github.com/AndrasKovacs/elaboration-zoo —
  branch `01-eval-closures-debruijn` is the minimal NbE + De Bruijn reference;
  later branches add metas/implicits.
- **staged demo** (Kovács): https://github.com/AndrasKovacs/staged/tree/main/demo —
  full 2LTT elaborator + stager; excerpted in [`demo-implementation.md`](demo-implementation.md). The repo also
  contains the LaTeX sources of both Kovács papers and an Agda embedding of CFTT
  (`icfp24paper/supplement`).
