# IR Design: Syntactic vs Semantic Types

## Overview

The compiler maintains two type representations:

- **Syntactic types** (`&Term`): Used in the elaborated IR, threaded through elaboration.
- **Semantic types** (`Value`): Computed during type checking via Normalization by Evaluation (NbE), used internally for dependent type checking.

This document explains the design rationale and when each is used.

## Design Decision: Thread Types Through Elaboration

**Current approach**: The elaborator threads types as semantic values, not as syntax.

**Rationale**:
- Most term variants carry enough information for type recovery (they are "self-typed").
- A few variants (`Var`, `Global`, `Lit`) require external context (locals, globals, or check-provided info).
- With NbE implemented, maintaining semantic types is natural and enables correct dependent type checking.
- The semantic domain (`Value`) is the authoritative type representation; quoting values to syntax is for error messages and output.

**Benefits**:
- No redundant type trees in the elaborated IR.
- Type correctness is maintained at elaboration time by the type checker.
- Simplifies dependent type checking: dependent arguments are checked by evaluating types as values and instantiating closures.

## Type Recovery and Quotation

For most term constructs, the type can be recovered from the term itself plus context:

- **Prims and literals**: Type is known from the primitive itself.
- **Lambdas and Pi types**: Type is the domain type (from parameters) or body type (via closure instantiation).
- **Applications**: Type is the return type of the callee (recovered via function type evaluation).
- **Lift/Quote/Splice**: Types follow definitional rules (Lift has meta universe type, Quote wraps in Lift, etc.).

When a syntactic type is needed (error messages, external APIs), it is produced by **quoting** semantic values back to terms.

## Relationship to Reference Implementation

The Kovács reference implementation uses a similar split:

- **`Tm`** (syntax): Post-elaboration IR with De Bruijn indices.
- **`Val`** (semantics): Result of evaluation, used for type checking.

Types flow through checking as semantic values; elaboration threads the elaborated term (not its type) forward.
