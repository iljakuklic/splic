---
name: 2ltt
description: Implement (or typecheck + stage) a practical two-level type theory (2LTT) / staged dependent type system, including a closure-free object language variant and optional representation/calling-convention indexing.
compatibility: opencode
---

## What I do

I'm a reference for implementing a **two-level type theory (2LTT)**: a dependently typed
meta-language over a staged object-language, connected via **lift/quote/splice**
(`⇑A` / `⟨t⟩` / `∼t`). I cover syntax, typing, definitional-equality design, elaboration,
and the staging algorithm that runs metaprograms to produce splice-free object code.

## Files (each owns its topic; minimal overlap)

1. **`implementation-guide.md`** — start here. Pipeline architecture, the up-front
   design decisions (object-equality strength, stage repair vs. reject), NbE core,
   elaboration guardrails, staging-pass essentials, pitfalls checklist, glossary.

2. **`demo-implementation.md`** — code-level walkthrough of the reference Haskell
   implementation (https://github.com/AndrasKovacs/staged): core syntax, conversion
   evaluator with stuck splices, two-domain staging evaluator, coercive-subtyping
   elaboration, unification extensions, what to copy vs. reconsider.

3. **`kovacs-2022-staged-compilation-2ltt.md`** — the core theory (*Staged Compilation
   with Two-Level Type Theory*): rules, programming patterns, binding-time improvement
   and inference, staging-by-evaluation with soundness/stability/strictness,
   object-language variations (monomorphization, representation polymorphism),
   intensional-analysis options. Also covers the ICFP'22 slides' examples.

4. **`kovacs-2024-closure-free-2ltt.md`** — the CFTT deltas (*Closure-Free Functional
   Programming in a 2LTT*): first-order object language with `ValTy`/`CompTy`, weak
   object equality, `Gen`/let-insertion, `Improve` monad library, join points + SOP,
   stream fusion, generativity axiom.

5. **`downen-2020-kinds-are-calling-conventions.md`** — optional layout/arity control:
   `TYPE ρ ν` kinds, levity, `mono-rep`/`mono-conv` restrictions, closure boxing,
   lowering to machine language; how it composes with a 2LTT.

## When to use me

- implementing or debugging a 2LTT core calculus with compile-time evaluation and
  typed splicing;
- designing an object language meant to compile predictably (optionally closure-free);
- adding representation/calling-convention indices for low-level codegen;
- reasoning about staging correctness (soundness/stability/strictness) or about what
  metaprograms can and cannot observe.

## Guardrails (important)

- **Conversion vs. staging are different evaluators.** Staging never β-reduces object
  code (strictness). Whether *conversion checking* computes object redexes depends on
  the chosen object theory: full-MLTT object level (2022) — yes; CFTT-style first-order
  object level — no β/η/let-unfolding at all. Pick one coherently
  (`implementation-guide.md` §2.1).
- **`infer` returns `(Term, VTy, Stage)`** — never reconstruct types (or stages) from
  elaborated terms afterwards.
- **`checkU`** — when a term must be a type, check it against `U s` directly; don't
  infer-and-test.
- **Stuck splices need a representation** in the semantic domain (spine entry or
  neutral), with cancellation in both directions: `∼⟨t⟩ = t` and `⟨∼n⟩ = n`. Otherwise
  splices are silently dropped or crash on neutrals.
- **Track a stage for every binding**; on mismatch either *repair* via coercive
  subtyping (`A ≤ ⇑A`, `⇑A ≤ A` — what the reference implementation does) or *reject*
  with a clear stage error — but decide explicitly. Keep `let` stages explicit; avoid
  stage metavariables.
- **Keep object code opaque** (build/compose/insert only — no pattern-matching on code)
  unless you deliberately adopt a weakening-only or closed-modality setup; opacity is
  what makes the standard semantics and the generativity axiom work.
