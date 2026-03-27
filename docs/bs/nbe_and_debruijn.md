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

### Evaluation (eval function)

```rust
pub fn eval<'a>(
    arena: &'a Bump,
    globals: &HashMap<Name<'a>, &'a Term<'a>>,
    env: &Env<'a>,
    term: &'a Term<'a>,
) -> Value<'a> {
    match term {
        // Variable: convert index to stack position
        Term::Var(Ix(i)) => {
            let stack_pos = env.len() - 1 - i;
            env[stack_pos].clone()
        }

        // Neutral references
        Term::Global(name) => Value::Global(*name),
        Term::Prim(p) => Value::Prim(*p),
        Term::Lit(n) => Value::Lit(*n),

        // Lambda: create closure by snapshotting environment
        Term::Lam(lam) if lam.params.is_empty() => {
            eval(arena, globals, env, lam.body)
        }
        Term::Lam(lam) => {
            let (name, ty) = lam.params[0];
            let param_ty = eval(arena, globals, env, ty);
            let rest_body = if lam.params.len() == 1 {
                lam.body
            } else {
                // Slice to remaining params (zero-copy)
                arena.alloc(Term::Lam(Lam {
                    params: &lam.params[1..],
                    body: lam.body,
                }))
            };
            Value::Lam(VLam {
                name,
                param_ty: Box::new(param_ty),
                closure: Closure {
                    env: arena.alloc_slice_fill_iter(env.iter().cloned()),
                    body: rest_body,
                },
            })
        }

        // Pi: similar to Lam
        Term::Pi(pi) if pi.params.is_empty() => {
            eval(arena, globals, env, pi.body_ty)
        }
        Term::Pi(pi) => {
            let (name, ty) = pi.params[0];
            let domain = eval(arena, globals, env, ty);
            let rest_body = if pi.params.len() == 1 {
                pi.body_ty
            } else {
                arena.alloc(Term::Pi(Pi {
                    params: &pi.params[1..],
                    body_ty: pi.body_ty,
                    phase: pi.phase,
                }))
            };
            Value::Pi(VPi {
                name,
                domain: Box::new(domain),
                closure: Closure {
                    env: arena.alloc_slice_fill_iter(env.iter().cloned()),
                    body: rest_body,
                },
                phase: pi.phase,
            })
        }

        // Application
        Term::App(app) => {
            let func_val = eval(arena, globals, env, app.func);
            let arg_vals: Vec<_> = app.args.iter()
                .map(|arg| eval(arena, globals, env, arg))
                .collect();
            apply_many(arena, globals, func_val, arg_vals)
        }

        // Let binding
        Term::Let(let_) => {
            let val = eval(arena, globals, env, let_.expr);
            let mut env2 = env.clone();
            env2.push(val);
            eval(arena, globals, &env2, let_.body)
        }

        // Quoted/lifted/spliced code
        Term::Quote(inner) => Value::Quote(Box::new(eval(arena, globals, env, inner))),
        Term::Lift(inner) => Value::Lift(Box::new(eval(arena, globals, env, inner))),
        Term::Splice(inner) => {
            // Splice unwraps a Quote; otherwise stays as application
            let inner_val = eval(arena, globals, env, inner);
            if let Value::Quote(inner) = inner_val {
                // Splice of a Quote: splice splices away to get the inner term, quote-splice
                // For now: pass through
                Value::Quote(*inner)
            } else {
                // Splice of non-quote: leave as application (staging will handle it)
                Value::App(Box::new(inner_val), &[])
            }
        }

        Term::Match(match_) => {
            let scrutinee_val = eval(arena, globals, env, match_.scrutinee);
            // Pattern match in semantic domain
            for arm in match_.arms {
                if matches_arm(&arm.pat, &scrutinee_val) {
                    match &arm.pat {
                        Pat::Bind(name) => {
                            let mut env2 = env.clone();
                            env2.push(scrutinee_val);
                            return eval(arena, globals, &env2, arm.body);
                        }
                        Pat::Lit(_) | Pat::Wildcard => {
                            return eval(arena, globals, env, arm.body);
                        }
                    }
                }
            }
            unreachable!("match non-exhaustive")
        }
    }
}
```

### Closure Instantiation (apply)

```rust
pub fn apply<'a>(
    arena: &'a Bump,
    globals: &HashMap<Name<'a>, &'a Term<'a>>,
    closure: &Closure<'a>,
    arg: Value<'a>,
) -> Value<'a> {
    let mut env = closure.env.to_vec();  // Clone snapshot back to mutable vector
    env.push(arg);
    eval(arena, globals, &env, closure.body)
}

pub fn apply_many<'a>(
    arena: &'a Bump,
    globals: &HashMap<Name<'a>, &'a Term<'a>>,
    func: Value<'a>,
    args: Vec<Value<'a>>,
) -> Value<'a> {
    match func {
        Value::Lam(vlam) => {
            let mut result = apply(arena, globals, &vlam.closure, args[0].clone());
            for arg in args.iter().skip(1) {
                if let Value::Lam(vlam) = result {
                    result = apply(arena, globals, &vlam.closure, arg.clone());
                } else {
                    // Not a lambda; application sticks
                    return Value::App(Box::new(result), arena.alloc_slice_copy(&args[1..]));
                }
            }
            result
        }
        other => Value::App(Box::new(other), arena.alloc_slice_copy(&args)),
    }
}
```

### Quotation (quote)

Convert values back to term syntax. Used for error reporting, output, and type comparison.

```rust
pub fn quote<'a>(
    arena: &'a Bump,
    depth: Lvl,
    val: &Value<'a>,
) -> &'a Term<'a> {
    match val {
        Value::Rigid(lvl) => {
            // Convert level to index
            let ix = lvl_to_ix(depth, *lvl);
            arena.alloc(Term::Var(ix))
        }

        Value::Global(name) => arena.alloc(Term::Global(*name)),
        Value::Prim(p) => arena.alloc(Term::Prim(*p)),
        Value::Lit(n) => arena.alloc(Term::Lit(*n)),

        Value::Lam(vlam) => {
            // Apply closure to fresh variable at current depth
            let fresh = Value::Rigid(depth);
            let body_val = apply(arena, globals, &vlam.closure, fresh);
            let body_term = quote(arena, depth.succ(), &body_val);

            // Recover param info from VLam
            let param_ty_term = quote(arena, depth, vlam.param_ty);
            arena.alloc(Term::Lam(Lam {
                params: arena.alloc_slice_copy(&[(vlam.name, param_ty_term)]),
                body: body_term,
            }))
        }

        Value::Pi(vpi) => {
            let fresh = Value::Rigid(depth);
            let body_val = apply(arena, globals, &vpi.closure, fresh);
            let body_term = quote(arena, depth.succ(), &body_val);

            let domain_term = quote(arena, depth, vpi.domain);
            arena.alloc(Term::Pi(Pi {
                params: arena.alloc_slice_copy(&[(vpi.name, domain_term)]),
                body_ty: body_term,
                phase: vpi.phase,
            }))
        }

        Value::App(func, args) => {
            let qfunc = quote(arena, depth, func);
            let qargs: Vec<_> = args.iter().map(|a| quote(arena, depth, a)).collect();
            arena.alloc(Term::App(App {
                func: qfunc,
                args: arena.alloc_slice_copy(&qargs),
            }))
        }

        Value::Lift(inner) => {
            arena.alloc(Term::Lift(quote(arena, depth, inner)))
        }

        Value::Quote(inner) => {
            arena.alloc(Term::Quote(quote(arena, depth, inner)))
        }
    }
}
```

## Type Checker Integration

The type checker (`checker/mod.rs`) maintains a context with an **evaluation environment**:

```rust
pub struct Ctx<'core, 'globals> {
    arena: &'core Bump,
    env: value::Env<'core>,          // evaluation environment (Values)
    types: Vec<value::Value<'core>>, // type of each local
    lvl: Lvl,                         // current depth
    names: Vec<&'core str>,           // names for error messages
    globals: &'globals HashMap<Name<'core>, &'core Term<'core>>,
}

impl<'core, 'globals> Ctx<'core, 'globals> {
    pub fn push_local(&mut self, name: &'core str, ty_val: value::Value<'core>) {
        self.env.push(value::Value::Rigid(self.lvl));  // variable at this level
        self.types.push(ty_val);
        self.names.push(name);
        self.lvl = self.lvl.succ();
    }

    pub fn pop_local(&mut self) {
        self.env.pop();
        self.types.pop();
        self.names.pop();
        self.lvl = Lvl(self.lvl.0 - 1);
    }

    pub fn type_of(&self, term: &Term) -> value::Value<'_> {
        match term {
            Term::Var(Ix(i)) => {
                let stack_pos = self.types.len() - 1 - i;
                self.types[stack_pos].clone()
            }
            // ... other cases
        }
    }
}
```

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
