//! Normalization by Evaluation (`NbE`) for the type checker.
//!
//! Types are maintained as semantic `Value`s; "substitution" is replaced by
//! environment extension + `eval`. `quote` converts values back to terms for
//! error reporting and definitional equality checking.

use bumpalo::Bump;

use super::prim::IntType;
use super::{Lam, Name, Pat, Pi, Prim, Term};
use crate::common::{Phase, de_bruijn};

/// Working evaluation environment: index 0 = outermost binding, last = innermost.
/// `Var(Ix(i))` maps to `env[env.len() - 1 - i]`.
pub type Env<'names, 'a> = Vec<Value<'names, 'a>>;

/// Semantic value — result of evaluating a term or type.
#[derive(Clone, Debug)]
pub enum Value<'names, 'a> {
    /// Neutral: stuck on a local variable (identified by De Bruijn level)
    Rigid(de_bruijn::Lvl),
    /// Neutral: global function reference (not inlined during type-checking)
    Global(&'names Name),
    /// Neutral: unapplied or partially applied primitive
    Prim(Prim),
    /// Neutral: stuck application (callee cannot reduce further)
    App(&'a Self, &'a [Self]),
    /// Canonical: integer literal value
    Lit(u64, IntType),
    /// Canonical: lambda abstraction
    Lam(VLam<'names, 'a>),
    /// Canonical: dependent function type
    Pi(VPi<'names, 'a>),
    /// Canonical: lifted object type `[[T]]`
    Lift(&'a Self),
    /// Canonical: quoted object code `#(t)`
    Quote(&'a Self),
    /// Neutral: stuck splice `$(t)` where `t` did not reduce to a Quote
    Splice(&'a Self),
}

/// Lambda value: parameter names/type-closures and body closure.
///
/// `params[i]` holds `(name, domain_closure)` where `domain_closure` captures
/// the base environment and, when instantiated with `args[0..i]`, produces the
/// type of the i-th parameter (supporting dependent telescopes).
/// `closure` takes all `params.len()` arguments.
#[derive(Clone, Debug)]
pub struct VLam<'names, 'a> {
    pub params: &'a [(&'names Name, Closure<'names, 'a>)],
    pub closure: Closure<'names, 'a>,
}

/// Pi (dependent function type) value.
///
/// Same telescope layout as `VLam`: `params[i].1` is instantiated with `args[0..i]`
/// to yield the domain of the i-th parameter. `ret_closure` takes all `params.len()` args.
#[derive(Clone, Debug)]
pub struct VPi<'names, 'a> {
    pub params: &'a [(&'names Name, Closure<'names, 'a>)],
    pub ret_closure: Closure<'names, 'a>,
    pub phase: Phase,
}

/// A closure: snapshot of the environment at creation time, plus an unevaluated body.
#[derive(Clone, Debug)]
pub struct Closure<'names, 'a> {
    /// Arena-allocated environment snapshot (index 0 = outermost).
    pub env: &'a [Value<'names, 'a>],
    /// Unevaluated body term.
    pub body: &'a Term<'names, 'a>,
}

/// Evaluate a term in an environment, producing a semantic value.
///
/// `env[env.len() - 1 - ix]` gives the value for `Var(Ix(ix))`.
pub fn eval<'names, 'a>(
    arena: &'a Bump,
    env: &[Value<'names, 'a>],
    term: &'a Term<'names, 'a>,
) -> Value<'names, 'a> {
    match term {
        Term::Var(ix) => {
            let lvl = ix.lvl_at(de_bruijn::Depth::new(env.len()));
            let i = lvl.as_usize();
            env.get(i)
                .expect("De Bruijn index out of environment bounds")
                .clone()
        }

        Term::Prim(p) => Value::Prim(*p),
        Term::Lit(n, it) => Value::Lit(*n, *it),
        Term::Global(name) => Value::Global(name),

        Term::Lam(lam) => eval_lam(arena, env, lam),
        Term::Pi(pi) => eval_pi(arena, env, pi),

        Term::App(app) => {
            let func_val = eval(arena, env, app.func);
            let arg_vals: Vec<Value<'names, 'a>> =
                app.args.iter().map(|a| eval(arena, env, a)).collect();
            apply_many(arena, func_val, &arg_vals)
        }

        Term::Lift(inner) => {
            let inner_val = eval(arena, env, inner);
            Value::Lift(arena.alloc(inner_val))
        }

        Term::Quote(inner) => {
            let inner_val = eval(arena, env, inner);
            match inner_val {
                Value::Splice(v) => (*v).clone(),
                v => Value::Quote(arena.alloc(v)),
            }
        }

        Term::Splice(inner) => {
            // Splice unwraps a Quote; otherwise stays stuck.
            match eval(arena, env, inner) {
                Value::Quote(v) => (*v).clone(),
                v => Value::Splice(arena.alloc(v)),
            }
        }

        Term::Let(let_) => {
            let val = eval(arena, env, let_.expr);
            let mut env2: Vec<Value<'names, 'a>> = env.to_vec();
            env2.push(val);
            eval(arena, &env2, let_.body)
        }

        Term::Match(match_) => {
            let scrut_val = eval(arena, env, match_.scrutinee);
            let n = match scrut_val {
                Value::Lit(n, _) => n,
                // Non-literal scrutinee: stuck, return neutral
                other => {
                    return Value::App(arena.alloc(other), &[]);
                }
            };
            for arm in match_.arms {
                match &arm.pat {
                    Pat::Lit(m) if n == *m => {
                        return eval(arena, env, arm.body);
                    }
                    Pat::Lit(_) => {}
                    Pat::Bind(_) | Pat::Wildcard => {
                        let mut env2 = env.to_vec();
                        // TODO(#24): Type should come from scrutinee, not hardcoded u64
                        env2.push(Value::Lit(
                            n,
                            IntType {
                                width: super::IntWidth::U64,
                                phase: Phase::Meta,
                            },
                        ));
                        return eval(arena, &env2, arm.body);
                    }
                }
            }
            // Non-exhaustive match (should not happen in well-typed code)
            unreachable!("non-exhaustive pattern match in match term evaluation")
        }
    }
}

/// Evaluate a multi-param Pi into a multi-param `Value::Pi` (no currying).
///
/// All parameter domain closures share the same base environment snapshot.
/// Each domain closure's body uses De Bruijn indices to reference preceding
/// parameters, so they are correctly differentiated despite sharing the base env.
pub fn eval_pi<'names, 'a>(
    arena: &'a Bump,
    env: &[Value<'names, 'a>],
    pi: &'a Pi<'names, 'a>,
) -> Value<'names, 'a> {
    let env_snapshot = arena.alloc_slice_fill_iter(env.iter().cloned());
    let params: Vec<(&'names Name, Closure<'names, 'a>)> = pi
        .params
        .iter()
        .map(|&(name, ty_term)| {
            (
                name,
                Closure {
                    env: env_snapshot,
                    body: ty_term,
                },
            )
        })
        .collect();
    Value::Pi(VPi {
        params: arena.alloc_slice_fill_iter(params),
        ret_closure: Closure {
            env: env_snapshot,
            body: pi.body_ty,
        },
        phase: pi.phase,
    })
}

/// Evaluate a multi-param Lam into a multi-param `Value::Lam` (no currying).
fn eval_lam<'names, 'a>(
    arena: &'a Bump,
    env: &[Value<'names, 'a>],
    lam: &'a Lam<'names, 'a>,
) -> Value<'names, 'a> {
    let env_snapshot = arena.alloc_slice_fill_iter(env.iter().cloned());
    let params: Vec<(&'names Name, Closure<'names, 'a>)> = lam
        .params
        .iter()
        .map(|&(name, ty_term)| {
            (
                name,
                Closure {
                    env: env_snapshot,
                    body: ty_term,
                },
            )
        })
        .collect();
    Value::Lam(VLam {
        params: arena.alloc_slice_fill_iter(params),
        closure: Closure {
            env: env_snapshot,
            body: lam.body,
        },
    })
}

/// Apply a value to multiple arguments simultaneously.
///
/// For `Value::Lam` and `Value::Pi`, all args are pushed into the closure env at once.
/// For neutrals, a stuck `App` node is produced. Each call site is its own `App` node;
/// args are NOT flattened into existing `App` nodes to preserve call-site identity.
pub fn apply_many<'names, 'a>(
    arena: &'a Bump,
    func: Value<'names, 'a>,
    args: &[Value<'names, 'a>],
) -> Value<'names, 'a> {
    match func {
        Value::Lam(vlam) => inst_n(arena, &vlam.closure, args),
        Value::Pi(vpi) => inst_n(arena, &vpi.ret_closure, args),
        Value::Rigid(lvl) => Value::App(
            arena.alloc(Value::Rigid(lvl)),
            arena.alloc_slice_fill_iter(args.iter().cloned()),
        ),
        Value::Global(name) => Value::App(
            arena.alloc(Value::Global(name)),
            arena.alloc_slice_fill_iter(args.iter().cloned()),
        ),
        Value::App(f, existing_args) => {
            // Nested application — do NOT flatten into existing args
            let callee = arena.alloc(Value::App(f, existing_args));
            Value::App(callee, arena.alloc_slice_fill_iter(args.iter().cloned()))
        }
        Value::Prim(p) => Value::App(
            arena.alloc(Value::Prim(p)),
            arena.alloc_slice_fill_iter(args.iter().cloned()),
        ),
        Value::Lit(..) | Value::Lift(_) | Value::Quote(_) | Value::Splice(_) => {
            // Should not happen in well-typed programs
            panic!("apply_many: function position holds non-function value")
        }
    }
}

/// Instantiate a closure with N arguments: extend env with all args, eval body.
pub fn inst_n<'names, 'a>(
    arena: &'a Bump,
    closure: &Closure<'names, 'a>,
    args: &[Value<'names, 'a>],
) -> Value<'names, 'a> {
    let mut env = closure.env.to_vec();
    env.extend_from_slice(args);
    eval(arena, &env, closure.body)
}

/// Quote a telescope (sequence of named parameters with closures).
/// Returns the quoted parameters, final depth, and the rigid values built during the process.
fn quote_telescope<'names, 'a>(
    arena: &'a Bump,
    initial_depth: de_bruijn::Depth,
    params: &[(&'names Name, Closure<'names, 'a>)],
) -> (
    Vec<(&'names Name, &'a Term<'names, 'a>)>,
    de_bruijn::Depth,
    Vec<Value<'names, 'a>>,
) {
    let mut rigid_vals = Vec::new();
    let mut quoted_params = Vec::new();
    let mut d = initial_depth;

    for (name, param_cl) in params {
        let param_val = inst_n(arena, param_cl, &rigid_vals);
        let param_term = quote(arena, d, &param_val);
        quoted_params.push((*name, param_term));
        rigid_vals.push(Value::Rigid(d.as_lvl()));
        d = d.succ();
    }

    (quoted_params, d, rigid_vals)
}

/// Convert a value back to a term (for error reporting and definitional equality).
///
/// `depth` is the current De Bruijn depth (number of locally-bound variables in scope).
pub fn quote<'names, 'a>(
    arena: &'a Bump,
    depth: de_bruijn::Depth,
    val: &Value<'names, 'a>,
) -> &'a Term<'names, 'a> {
    match val {
        Value::Rigid(lvl) => {
            let ix = lvl.ix_at(depth);
            arena.alloc(Term::Var(ix))
        }
        Value::Global(name) => arena.alloc(Term::Global(name)),
        Value::Prim(p) => arena.alloc(Term::Prim(*p)),
        Value::Lit(n, it) => arena.alloc(Term::Lit(*n, *it)),
        Value::App(f, args) => {
            let qf = quote(arena, depth, f);
            let qargs: Vec<&'a Term<'names, 'a>> = args
                .iter()
                .map(|a| quote(arena, depth, a) as &'a _)
                .collect();
            arena.alloc(Term::new_app(qf, arena.alloc_slice_fill_iter(qargs)))
        }
        Value::Lam(vlam) => {
            let (quoted_params, final_d, rigid_vals) = quote_telescope(arena, depth, vlam.params);
            let body_val = inst_n(arena, &vlam.closure, &rigid_vals);
            let body_term = quote(arena, final_d, &body_val);
            let params_slice = arena.alloc_slice_fill_iter(quoted_params);
            arena.alloc(Term::Lam(Lam {
                params: params_slice,
                body: body_term,
            }))
        }
        Value::Pi(vpi) => {
            let (quoted_params, final_d, rigid_vals) = quote_telescope(arena, depth, vpi.params);
            let ret_val = inst_n(arena, &vpi.ret_closure, &rigid_vals);
            let ret_term = quote(arena, final_d, &ret_val);
            let params_slice = arena.alloc_slice_fill_iter(quoted_params);
            arena.alloc(Term::Pi(Pi {
                params: params_slice,
                body_ty: ret_term,
                phase: vpi.phase,
            }))
        }
        Value::Lift(inner) => {
            let inner_term = quote(arena, depth, inner);
            arena.alloc(Term::Lift(inner_term))
        }
        Value::Quote(inner) => {
            let inner_term = quote(arena, depth, inner);
            arena.alloc(Term::Quote(inner_term))
        }
        Value::Splice(inner) => {
            let inner_term = quote(arena, depth, inner);
            arena.alloc(Term::Splice(inner_term))
        }
    }
}

/// Definitional equality: quote both values and compare structurally.
pub fn val_eq<'names, 'a>(
    depth: de_bruijn::Depth,
    a: &Value<'names, 'a>,
    b: &Value<'names, 'a>,
) -> bool {
    let scratch = Bump::new();
    let ta = quote(&scratch, depth, a);
    let tb = quote(&scratch, depth, b);
    super::alpha_eq::alpha_eq(ta, tb)
}

/// Evaluate a term in the empty environment.
pub fn eval_closed<'names, 'a>(arena: &'a Bump, term: &'a Term<'names, 'a>) -> Value<'names, 'a> {
    eval(arena, &[], term)
}

/// Extract the Phase from a Value that represents a universe (Type or `VmType`),
/// if it is indeed a type universe.
pub const fn value_phase(val: &Value<'_, '_>) -> Option<Phase> {
    match val {
        Value::Prim(Prim::IntTy(it)) => Some(it.phase),
        Value::Prim(Prim::U(_)) | Value::Lift(_) | Value::Pi(_) => Some(Phase::Meta),
        Value::Rigid(_)
        | Value::Global(_)
        | Value::App(_, _)
        | Value::Prim(_)
        | Value::Lit(..)
        | Value::Lam(_)
        | Value::Quote(_)
        | Value::Splice(_) => None,
    }
}
