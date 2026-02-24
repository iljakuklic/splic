---
name: 2ltt
description: Implement (or typecheck + unstage) a practical two-level type theory (2LTT) / staged dependent type system, including a closure-free object language variant and optional representation/calling-convention indexing.
compatibility: opencode
---

## What I do

I'm a project-local reference for implementing a **two-level type theory (2LTT)**: a dependently typed meta-language with a staged object-language, connected via **lift/quote/splice**.

I focus on implementation details: syntax, typing, definitional equality choices, and an *unstaging* (staging) algorithm that runs metaprograms to produce splice-free object code.

## Files to read (in order)

1. **`implementation-guide.md`**
   - Practical "how to build it" plan: core judgments, AST design, typechecker structure, definitional equality strategy, and an executable unstager.
   - Includes pseudocode and implementation checklists.

2. **`demo-implementation.md`**
   - Code snippets from the reference Haskell implementation at https://github.com/AndrasKovacs/staged
   - Syntax, semantic values, meta evaluation, and staging algorithm.
   - Useful for seeing how the theory maps to actual code.

3. **`kovacs-2022-staged-compilation-2ltt.md`**
   - Rewrites the main content of *Staged Compilation with Two-Level Type Theory* (Kovács 2022):
     universes-by-stage, lift/quote/splice rules, and *staging-by-evaluation* (presheaf-model-inspired) as an algorithmic blueprint.

4. **`kovacs-2024-closure-free-2ltt.md`**
   - Rewrites the main content of *Closure-Free Functional Programming in a Two-Level Type Theory* (Kovács 2024):
     a closure-free object language split into **value types vs computation types**, call-saturation idea, let-insertion via a codegen monad, and the "no intensional code analysis" / generativity theme.

5. **`downen-2020-kinds-are-calling-conventions.md`**
   - Rewrites the parts of *Kinds Are Calling Conventions* (Downen et al. 2020) that are useful when you want the staged object language to control **representation**, **arity**, and **evaluation order** via kind/index information.
   - Treat this as an optional extension for "layout control" / low-level compilation friendliness.

## When to use me

Use me when you are:
- implementing a 2LTT core calculus (or a prototype) with **compile-time evaluation** and **typed splicing**;
- designing an object language meant to compile efficiently (optionally closure-free);
- adding representation/calling-convention indices to types/kinds for lower-level codegen.

## Guardrails (important)

- Prefer an object-level definitional equality that is **simple and syntax-directed** (often: *no* β/η for object lambdas), so typechecking and unstaging stay predictable.
- Unstaging should be an evaluation procedure: **run meta**, construct **object AST**, eliminate splices.
- Avoid "intensional analysis of object code" (pattern matching on AST) if you want the semantic/parametricity properties these papers rely on.
