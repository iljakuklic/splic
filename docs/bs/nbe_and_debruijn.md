# Normalization by Evaluation and De Bruijn Representation

This document explains the type checker's use of **Normalization by Evaluation (NbE)** and the **De Bruijn index/level** variable representation. These are the mechanisms that replace syntactic substitution and enable correct handling of dependent types.

## Problem: Syntactic Substitution with Binders

Naive substitution fails when the replacement contains binders. Variables within the replacement use indices relative to that context; when spliced elsewhere, those indices are meaningless, causing capture bugs. This is critical for dependent type checking, which requires computing return types via substitution (e.g., `fn(x: A) -> B` with argument `arg` must produce `B[arg/x]`).

## Solution: Normalization by Evaluation

Instead of rewriting terms syntactically, **evaluate terms in an environment**. The environment tracks what each De Bruijn index refers to, eliminating the need for explicit substitution.

### De Bruijn Indices vs Levels

Two complementary representations:

**De Bruijn Indices:** Used in **term syntax**. An index counts from the nearest binder (0 = innermost). This is pure syntax, portable, and requires no external state to interpret.

**De Bruijn Levels:** Used internally in the **semantic domain**. A level counts from the outermost binder (0 = root) and grows monotonically during evaluation, making fresh variable generation natural.

**Conversions:**
```rust
ix_to_lvl(depth, ix) = Lvl(depth - ix - 1)
lvl_to_ix(depth, lvl) = Ix(depth - lvl - 1)
```

## Core NbE Design

The semantic domain separates values from syntax. **Closures** are the key structure: a closure pairs an **environment snapshot** (immutable slice) with an **unevaluated term**. When instantiated, the environment is extended and the body is evaluated.

**Evaluation** interprets terms in an environment. Variables are looked up in the environment, lambdas and Pi types create closures by snapshotting the environment, and applications instantiate closures.

**Quotation** converts values back to syntax for error reporting, type output, and comparison. Rigid variables are converted from levels back to indices; lambdas and Pi types are reconstructed by applying the closure to a fresh variable and recursively quoting the result.

## Type Checker Integration

The type checker maintains a context with an evaluation environment (values indexed by De Bruijn level), type environment (semantic types of bound variables), current depth, and globals table.

For dependent type checking, instead of syntactic substitution (which has capture bugs), the checker instantiates Pi closures: it evaluates the domain type, checks the argument, then applies the closure to the evaluated argument to get the return type.

## Code Value Index Shifting (Staging)

When quoted object code is stored with its creation depth and later spliced at a different depth, its free variable indices must be shifted. If code was created at depth N and spliced at depth M, free indices (those not bound by the code itself) are shifted by M - N. Only free variables are shifted; variables bound within the code are unaffected.

## See Also

- **Reference Implementation:** elaboration-zoo, branch 01-eval-closures-debruijn
- **Paper:** Kovács 2022, Staged Compilation with Two-Level Type Theory (ICFP)
- **Related:** [pi_types.md](pi_types.md) for grammar and examples
