# Normalization by Evaluation and De Bruijn Representation

This document explains the type checker's use of **Normalization by Evaluation (NbE)** and the **De Bruijn index/level** variable representation. These are the mechanisms that replace syntactic substitution and enable correct handling of dependent types.

## Problem: Syntactic Substitution with Binders

Naive substitution fails when the replacement contains binders:

```rust
// Example: substitute a lambda into another context
let replacement = Lam { param_name: "x", body: Var(Ix(0)) };  // the identity lambda |x| x
let target = Let {
    expr: Prim(U64),
    body: Var(Ix(0))  // refers to the let-bound variable
};

// Naive subst(target, position_of_0, replacement) would replace Var(0) with the Lam.
// But the Lam's body (Var(0)) refers to its own parameter (scope relative to the Lam),
// not the let-bound variable. This is a capture bug.
```

**Root cause:** The `Var(Ix)` in `replacement` uses indices relative to the Lam's scope, but when substituted elsewhere, those indices become meaningless. The two-level namespace problem (variable name vs. scope) requires careful handling.

**Why it matters:** Dependent function types need substitution to compute return types:
```
fn(x: A) -> B   with argument arg   =>   B[arg/x]
```

Without correct substitution (or an alternative), dependent type checking is broken.

## Solution: Normalization by Evaluation

Instead of rewriting terms syntactically, **evaluate terms in an environment**. The environment tracks what each De Bruijn index refers to, so substitution happens implicitly via environment extension.

### De Bruijn Indices vs Levels

Two complementary representations:

**De Bruijn Indices (`Ix`):** Used in **term syntax**. An index is **relative to the nearest binder** (0 = innermost).

```
\x . \y . x        -->  |x: _| |y: _| Var(Ix(1))
                        The 'x' is 1 step up from the innermost binder (y).

[(x, v1), (y, v2)]   at depth 2
Var(Ix(1)) ==> look up stack[2 - 1 - 1] = stack[0] = (x, v1)
```

**De Bruijn Levels (`Lvl`):** Used internally in **semantic domain and context**. A level is **absolute** (0 = outermost).

```
[x, y]    at depth 2
x is at level 0, y is at level 1
Fresh variable would be at level 2.
```

### Conversions

```rust
// Given current depth (how many binders we're under):
ix_to_lvl(depth: Lvl, ix: Ix) -> Lvl {
    Lvl(depth.0 - ix.0 - 1)
}

lvl_to_ix(depth: Lvl, lvl: Lvl) -> Ix {
    Ix(depth.0 - lvl.0 - 1)
}
```

**Why split the representation?**
- **Term syntax uses indices** — they are pure, no external state needed to interpret.
- **Semantics uses levels** — they grow monotonically as evaluation descends under binders, making fresh variable generation natural.
- This matches elaboration-zoo and Kovács' reference implementations.

## Core NbE Data Structures

### Value Domain (core/value.rs)

```rust
pub type Env<'a> = Vec<Value<'a>>;

pub enum Value<'a> {
    // Neutrals (stuck, cannot reduce further)
    Rigid(Lvl),                          // free variable
    Global(&'a str),                     // global function (not inlined)
    Prim(Prim),                          // primitive
    App(&'a Value<'a>, &'a [Value<'a>]), // application

    // Canonical forms
    Lit(u64),
    Lam(VLam<'a>),
    Pi(VPi<'a>),
    Lift(&'a Value<'a>),
    Quote(&'a Value<'a>),
}

pub struct VLam<'a> {
    pub name: &'a str,
    pub param_ty: &'a Value<'a>,
    pub closure: Closure<'a>,
}

pub struct VPi<'a> {
    pub name: &'a str,
    pub domain: &'a Value<'a>,
    pub closure: Closure<'a>,
    pub phase: Phase,
}

pub struct Closure<'a> {
    pub env: &'a [Value<'a>],  // immutable snapshot of environment
    pub body: &'a Term<'a>,    // unevaluated body
}
```

**Key insight:** Closures pair an **environment snapshot** (arena-allocated slice) with an **unevaluated term**. When instantiated, the environment is extended and the body is evaluated.

### Evaluation

The evaluator interprets terms in an environment, producing semantic values:

**Key principles**:
- **Variables** are converted from indices to stack positions via environment lookup.
- **Lambdas and Pi types** create closures by snapshotting the current environment.
- **Applications** apply a function value to arguments, evaluating on-demand.
- **Let bindings** are eagerly evaluated: extend the environment with the let-bound value and continue.
- **Lift/Quote/Splice** are reduced according to staging rules.
- **Match** performs pattern matching at the semantic level.

**Variadic handling**: Multi-parameter lambdas and Pi types are curried by slicing, not duplicating:
remaining parameters are encoded as a sub-term, avoiding allocation.

### Closure Instantiation

To apply a closure to an argument:

1. Restore the closure's environment from its snapshot.
2. Extend it with the argument value.
3. Evaluate the body in the extended environment.

For multi-argument applications, apply repeatedly: each application produces a value that may itself be a lambda (closure), ready for the next argument.

If application gets stuck (callee is not a lambda), the application becomes neutral (`Value::App`).

### Quotation

Convert values back to term syntax. Used for error reporting, output, and type comparison.

**Key operations**:
- **Rigid variables** are converted from levels to indices using `lvl_to_ix(depth, lvl)`.
- **Globals, prims, and literals** are reconstructed directly.
- **Lambdas and Pi types** are reconstructed by:
  1. Applying the closure to a fresh variable at the current depth.
  2. Recursively quoting the result at the next depth.
  3. Storing the original parameter information (name, type).
- **Applications** are reconstructed by quoting the function and arguments.
- **Lift/Quote/Splice** are reconstructed structurally.

## Type Checker Integration

The type checker maintains a context with:

- **Evaluation environment** (`env`): Values of bound variables, indexed by De Bruijn level.
- **Type environment** (`types`): Semantic type of each bound variable.
- **Current depth** (`lvl`): How many binders we are under.
- **Name tracking** (`names`): Variable names for error messages (not used in lookups).
- **Globals table** (`globals`): Top-level function types.

When a local variable is bound:
1. Push a fresh rigid value at the current level.
2. Push its type (as a semantic value) into the type environment.
3. Increment the depth.

This way, type information flows as semantic values throughout checking, and variable lookup is O(1) indexing.

### Dependent Type Checking Example

```rust
// Check a multi-argument application
fn check_app(ctx: &Ctx, app: &App) -> Result<Value> {
    let func_type = type_of(ctx, app.func)?;

    let mut pi_val = func_type;
    let mut checked_args = Vec::new();

    for arg_term in app.args {
        let Value::Pi(vpi) = pi_val else {
            bail!("too many arguments");
        }

        // Check argument against domain
        check(ctx, arg_term, &vpi.domain)?;
        let arg_val = eval(ctx.arena, ctx.globals, &ctx.env, arg_term);
        checked_args.push(arg_term);

        // Advance return type by instantiating closure
        pi_val = apply(ctx.arena, ctx.globals, &vpi.closure, arg_val);
    }

    Ok(pi_val)  // return type
}
```

No syntactic substitution; the dependent return type is computed by evaluating the Pi closure.

## Code Value Index Shifting (Staging)

When quoted object code is stored as `MetaVal::Code` and later reused in a different context, its De Bruijn indices must be adjusted.

### The Problem

```rust
// Code generated at depth 2 (x1 is Ix(0), x0 is Ix(1))
let code = MetaVal::Code {
    term: App { func: Global("mul"), args: [Var(Ix(0)), Var(Ix(1))] },
    depth: 2,
};

// Later, code is spliced at depth 4 (x3, x2, x1, x0 from innermost)
// Ix(0) still refers to x3 (innermost), not x1
// Ix(1) still refers to x2, not x0
// The indices are wrong!
```

### Solution: Shift Free Indices

Store the creation depth with the code value. On splice, shift free variable indices:

```rust
fn shift_free_ix<'out>(
    arena: &'out Bump,
    term: &'out Term<'out>,
    shift: usize,
    cutoff: usize,
) -> &'out Term<'out> {
    match term {
        Term::Var(Ix(i)) => {
            if i >= cutoff {
                arena.alloc(Term::Var(Ix(i + shift)))
            } else {
                arena.alloc(term.clone())
            }
        }
        Term::Lam(lam) => {
            // Shift continues into the body (free vars are those >= cutoff)
            let new_body = shift_free_ix(arena, lam.body, shift, cutoff);
            // ... reconstruct lam
        }
        // ... other cases recursively apply shift
    }
}

// On splice:
let depth_delta = current_depth - creation_depth;
let shifted_term = shift_free_ix(arena, code_term, depth_delta, 0);
```

**Key insight:** Only "free" variables (those at `Ix >= cutoff`) are shifted. Bound variables (introduced by the code itself) are not affected — only the indices in the code that refer to enclosing context.

## See Also

- **Reference Implementation:** elaboration-zoo, branch 01-eval-closures-debruijn
- **Paper:** Kovács 2022, Staged Compilation with Two-Level Type Theory (ICFP)
- **Related:** [pi_types.md](pi_types.md) for grammar and examples
