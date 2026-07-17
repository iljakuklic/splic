# 2LTT Conformance Audit — 2026-07-17

A point-in-time review of the Splic implementation against the two-level type
theory literature and reference implementation (Kovács 2022 *Staged Compilation
with 2LTT*, Kovács 2024 *Closure-Free Functional Programming in a 2LTT*, and the
`AndrasKovacs/staged` demo elaborator), as distilled in the project's `2ltt`
skill. Code references are to the state at commit `ade1876`; they will drift.

Headline: the **staging pass conforms well**; the substantive problems are in
the **checker's NbE/conversion layer**. Two soundness holes were confirmed with
runnable counterexamples, and several meta-level conversion gaps block the
staged-programming patterns the language is built around.

## Confirmed soundness bugs

### 1. Stuck `match` loses its arms in the semantic domain → unsound conversion

Tracked in [#74](https://github.com/iljakuklic/splic/issues/74) (severity
upgraded by this audit — previously believed not to affect correctness).

`value::eval` represents a stuck match as `Value::App(scrutinee, [])`,
discarding the arms; `quote` then renders it as the bogus term `b()`. Since
`val_eq` is quote-then-compare, *any two* dependent match types over the same
scrutinee are convertible. Accepted counterexample:

```splic
def orig(b: u1) -> (match b { 0 => u0, 1 => u16 }) = match b { 0 => 0, 1 => 42 };
def bad(b: u1)  -> (match b { 0 => u16, 1 => u0 }) = orig(b);   -- arms swapped
code def boom() -> u0 = { $(bad(1)) };
```

stages to `code def boom() -> u0 = 42_u0;` — the value 42 at a type that only
holds 0. The theory-side lesson (2ltt skill guardrails): every stuck
eliminator needs a faithful neutral representation; the reference
implementation carries stuck `NatElim` in value spines for exactly this reason.
The same collapse also equates object matches under quotes during conversion,
and `alpha_eq`'s match case (which wrongly compares binder names) becomes
reachable once the neutral is added.

### 2. Phase not tracked per binding, not checked at use sites → ICE on accepted programs

Tracked in [#27](https://github.com/iljakuklic/splic/issues/27) (previously
framed as an error-message-quality improvement; it is a soundness gap).

`CtxEntry` stores no phase and `infer` never compares a variable's phase with
the ambient one. Phase discipline instead leaks through phase-indexed types,
which fails wherever `infer` demands only "some integer type": match
scrutinees and comparison operands. Both of these elaborate and then panic in
staging with "(typechecker invariant)" messages:

```splic
code def f(x: u64) -> u64 = { $( match x { 0 => #(1), _ => #(2) } ) };
-- object variable in meta context (staging/mod.rs Var case, meta evaluator)

def g(n: u64) -> [[u64]] = #( match n { 0 => 1, _ => 2 } );
code def h() -> u64 = { $(g(3)) };
-- meta variable in object context (staging/mod.rs Var case, object evaluator)
```

The first is notable because it is the naive rendering of the CFTT "trick"
(case-splitting on object code) — it deserves a targeted error, not a crash.
The skill's guardrail: record a stage for every binding and, at use sites,
either *repair* (coercive subtyping `A ≤ ⇑A`, `⇑A ≤ A`, as the reference
implementation does via `adjustStage`/`coe` — it has no `guardStage` reject)
or *reject* with a clear stage error — but decide explicitly. Reject is the
simpler fit for Splic. Global lookups need the same check.
`docs/bs/prototype_eval.md`'s claim that "phase invariants are enforced" at
elaboration is inaccurate until this is fixed.

## Meta-level conversion is weaker than 2LTT requires

The 2022 design requires full βδ(η) conversion at the meta level — type-level
metaprogramming (`Vec : Nat1 → ⇑U0 → ⇑U0` and friends) is the flagship
pattern. Current gaps, each of which blocks that pattern independently:

- **No primitive computation in conversion** — `apply_many` leaves all `Prim`
  applications stuck, so `1 + 1` never equals `2` in a type. The checker's
  evaluator is strictly weaker than the staging evaluator at the meta level,
  which is backwards. [#48](https://github.com/iljakuklic/splic/issues/48)
- **No δ-unfolding of meta globals** — `Value::Global` is a permanent neutral,
  so `sel(0)` is never convertible to its unfolding.
  [#110](https://github.com/iljakuklic/splic/issues/110)
- **Signatures can't reference globals at all** — `elaborate_sig` runs with an
  empty globals table. [#72](https://github.com/iljakuklic/splic/issues/72)
- **Inference coverage** — match scrutinees and simple-`let` annotations go
  through `infer`, which rejects literals, arithmetic, and `match`; the
  simple-`let` annotation path should use `check_universe` like the
  parameterized path does.
  [#113](https://github.com/iljakuklic/splic/issues/113)
- **No η for meta functions** — quote-then-α-compare gives β but not η. Minor,
  sound, noted on [#48](https://github.com/iljakuklic/splic/issues/48).

## Missing capability: abstraction over object types

`VmType` is rejected by name in meta contexts, and `[[VmType]]` is ill-typed
(`VmType : Type`, not `: VmType`), so no metaprogram can quantify over object
types — the central 2022 pattern (`map : (A B : ⇑U0) → …`) is inexpressible.
Splic's `VmType : Type` already *is* the 2022 §2.5.1 monomorphization design
(`Ty0 : U1`, object types as meta data); allowing `VmType` as a meta type and
staging object-type positions for real is the consistent completion of that
design. [#111](https://github.com/iljakuklic/splic/issues/111)

## Undecided design point: object definitional equality strength

Conversion currently unfolds object `let`s and reduces object matches on
literals (2022-style full object equality), while the object language is
CFTT-shaped (first-order, object types never depend on object terms) and the
roadmap adds object-level recursion (`functional_goto.md`) — under which
conversion-time unfolding stops terminating. The skill's §2.1 warning applies:
pick one point coherently. Recommendation: CFTT-style weak object equality (no
object let-unfolding/β in conversion). Staging is already correctly strict
either way. [#112](https://github.com/iljakuklic/splic/issues/112)

## Minor observations

- No stability/strictness property tests for staging (stage of splice-free
  code is identity; object redexes survive verbatim).
  [#114](https://github.com/iljakuklic/splic/issues/114)
- No `tQuote`/`tSplice` smart constructors on elaborated terms; cancellation
  exists only at the value level. Cosmetic (smaller core terms).
- `TODO(#24)`: match-bound variables get a hardcoded meta-`u64` value in
  `eval` — also silently mis-phases object scrutinees.
- `Prim::Embed` does not range-mask its argument; unreachable with well-typed
  input today, but worth an assert once the soundness holes close.

## What conforms (verified, no action needed)

- **Staging pass** (`staging/mod.rs`): separate pass; two value domains; meta
  types erased to a dummy (`MetaVal::Ty`); De Bruijn levels in object values so
  splicing needs no shifting; `eval_obj` structural (strictness holds);
  invariant violations are panics per project policy.
- **Checker NbE**: stuck splices representable (`Value::Splice`) with
  quote/splice cancellation in both directions; `check_universe` follows the
  `checkU` pattern; local `let` values unfold via the environment.
- **Language shape**: explicit phases on global `def`s (no phase inference
  ambiguity); no metavariables, so zonking is correctly a non-issue;
  `Prim::Embed` is a sound serialization primitive (the `Nat1 → ⇑Nat0`
  direction from the 2022 paper).
