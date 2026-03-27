//! Normalization by Evaluation (`NbE`) for the type checker.
//!
//! Types are maintained as semantic `Value`s; "substitution" is replaced by
//! environment extension + `eval`. `quote` converts values back to terms for
//! error reporting and definitional equality checking.

use bumpalo::Bump;

use super::prim::IntType;
use super::{Lam, Lvl, Name, Pat, Pi, Prim, Term};
use crate::common::Phase;

/// Working evaluation environment: index 0 = outermost binding, last = innermost.
/// `Var(Ix(i))` maps to `env[env.len() - 1 - i]`.
pub type Env<'a> = Vec<Value<'a>>;

/// Semantic value — result of evaluating a term or type.
#[derive(Clone, Debug)]
pub enum Value<'a> {
    /// Neutral: stuck on a local variable (identified by De Bruijn level)
    Rigid(Lvl),
    /// Neutral: global function reference (not inlined during type-checking)
    Global(Name<'a>),
    /// Neutral: unapplied or partially applied primitive
    Prim(Prim),
    /// Neutral: stuck application (callee cannot reduce further)
    App(&'a Self, &'a [Self]),
    /// Canonical: integer literal value
    Lit(u64, IntType),
    /// Canonical: lambda abstraction
    Lam(VLam<'a>),
    /// Canonical: dependent function type
    Pi(VPi<'a>),
    /// Canonical: lifted object type `[[T]]`
    Lift(&'a Self),
    /// Canonical: quoted object code `#(t)`
    Quote(&'a Self),
    /// Canonical: universe `Type` or `VmType`
    U(Phase),
}

/// Lambda value: parameter name, parameter type, and body closure.
#[derive(Clone, Debug)]
pub struct VLam<'a> {
    pub name: &'a str,
    pub param_ty: &'a Value<'a>,
    pub closure: Closure<'a>,
}

/// Pi (dependent function type) value.
#[derive(Clone, Debug)]
pub struct VPi<'a> {
    pub name: &'a str,
    pub domain: &'a Value<'a>,
    pub closure: Closure<'a>,
    pub phase: Phase,
}

/// A closure: snapshot of the environment at creation time, plus an unevaluated body.
#[derive(Clone, Debug)]
pub struct Closure<'a> {
    /// Arena-allocated environment snapshot (index 0 = outermost).
    pub env: &'a [Value<'a>],
    /// Unevaluated body term.
    pub body: &'a Term<'a>,
}

/// Evaluate a term in an environment, producing a semantic value.
///
/// `env[env.len() - 1 - ix]` gives the value for `Var(Ix(ix))`.
pub fn eval<'a>(arena: &'a Bump, env: &[Value<'a>], term: &'a Term<'a>) -> Value<'a> {
    match term {
        Term::Var(ix) => {
            let i = env
                .len()
                .checked_sub(1 + ix.0)
                .expect("De Bruijn index out of environment bounds");
            env.get(i)
                .expect("De Bruijn index out of environment bounds")
                .clone()
        }

        Term::Prim(p) => Value::Prim(*p),
        Term::Lit(n, it) => Value::Lit(*n, *it),
        Term::Global(name) => Value::Global(*name),

        Term::Lam(lam) => eval_lam(arena, env, lam),
        Term::Pi(pi) => eval_pi(arena, env, pi),

        Term::App(app) => {
            let func_val = eval(arena, env, app.func);
            let arg_vals: Vec<Value<'a>> =
                app.args.iter().map(|a| eval(arena, env, a)).collect();
            apply_many(arena, func_val, &arg_vals)
        }

        Term::Lift(inner) => {
            let inner_val = eval(arena, env, inner);
            Value::Lift(arena.alloc(inner_val))
        }

        Term::Quote(inner) => {
            let inner_val = eval(arena, env, inner);
            Value::Quote(arena.alloc(inner_val))
        }

        Term::Splice(inner) => {
            // In type-checking context: splice unwraps a Quote, otherwise propagates.
            match eval(arena, env, inner) {
                Value::Quote(v) => (*v).clone(),
                v => v,
            }
        }

        Term::Let(let_) => {
            let val = eval(arena, env, let_.expr);
            let mut env2: Vec<Value<'a>> = env.to_vec();
            env2.push(val);
            eval(arena, &env2, let_.body)
        }

        Term::Match(match_) => {
            let scrut_val = eval(arena, env, match_.scrutinee);
            let n = match scrut_val {
                Value::Lit(n, _) => n,
                // Non-literal scrutinee: stuck, return neutral
                other => {
                    return Value::App(arena.alloc(other), arena.alloc_slice_fill_iter([]));
                }
            };
            for arm in match_.arms {
                match &arm.pat {
                    Pat::Lit(m) if n == *m => {
                        return eval(arena, env, arm.body);
                    }
                    Pat::Lit(_) => continue,
                    Pat::Bind(_) | Pat::Wildcard => {
                        let mut env2 = env.to_vec();
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
            Value::Rigid(Lvl(usize::MAX))
        }
    }
}

/// Evaluate a multi-param Pi, currying by slicing.
pub fn eval_pi<'a>(arena: &'a Bump, env: &[Value<'a>], pi: &'a Pi<'a>) -> Value<'a> {
    match pi.params {
        [] => eval(arena, env, pi.body_ty),
        [(name, ty), rest @ ..] => {
            let domain = eval(arena, env, ty);
            let rest_body: &'a Term<'a> = if rest.is_empty() {
                pi.body_ty
            } else {
                arena.alloc(Term::Pi(Pi {
                    params: rest,
                    body_ty: pi.body_ty,
                    phase: pi.phase,
                }))
            };
            let closure = Closure {
                env: arena.alloc_slice_fill_iter(env.iter().cloned()),
                body: rest_body,
            };
            Value::Pi(VPi {
                name,
                domain: arena.alloc(domain),
                closure,
                phase: pi.phase,
            })
        }
    }
}

/// Evaluate a multi-param Lam, currying by slicing.
fn eval_lam<'a>(arena: &'a Bump, env: &[Value<'a>], lam: &'a Lam<'a>) -> Value<'a> {
    match lam.params {
        [] => eval(arena, env, lam.body),
        [(name, ty), rest @ ..] => {
            let param_ty = eval(arena, env, ty);
            let rest_body: &'a Term<'a> = if rest.is_empty() {
                lam.body
            } else {
                arena.alloc(Term::Lam(Lam {
                    params: rest,
                    body: lam.body,
                }))
            };
            let closure = Closure {
                env: arena.alloc_slice_fill_iter(env.iter().cloned()),
                body: rest_body,
            };
            Value::Lam(VLam {
                name,
                param_ty: arena.alloc(param_ty),
                closure,
            })
        }
    }
}

/// Apply a single argument to a value.
pub fn apply<'a>(arena: &'a Bump, func: Value<'a>, arg: Value<'a>) -> Value<'a> {
    match func {
        Value::Lam(vlam) => inst(arena, &vlam.closure, arg),
        Value::Pi(vpi) => inst(arena, &vpi.closure, arg),
        Value::Rigid(lvl) => Value::App(
            arena.alloc(Value::Rigid(lvl)),
            arena.alloc_slice_fill_iter([arg]),
        ),
        Value::Global(name) => Value::App(
            arena.alloc(Value::Global(name)),
            arena.alloc_slice_fill_iter([arg]),
        ),
        Value::App(f, args) => {
            let mut new_args: Vec<Value<'a>> = args.to_vec();
            new_args.push(arg);
            Value::App(f, arena.alloc_slice_fill_iter(new_args))
        }
        Value::Prim(p) => Value::App(
            arena.alloc(Value::Prim(p)),
            arena.alloc_slice_fill_iter([arg]),
        ),
        Value::Lit(..) | Value::Lift(_) | Value::Quote(_) | Value::U(_) => {
            // Should not happen in well-typed programs
            panic!("apply: function position holds non-function value")
        }
    }
}

/// Apply a value to multiple arguments in sequence.
pub fn apply_many<'a>(arena: &'a Bump, func: Value<'a>, args: &[Value<'a>]) -> Value<'a> {
    args.iter()
        .fold(func, |f, arg| apply(arena, f, arg.clone()))
}

/// Instantiate a closure with one argument: extend env with arg, eval body.
pub fn inst<'a>(arena: &'a Bump, closure: &Closure<'a>, arg: Value<'a>) -> Value<'a> {
    let mut env = closure.env.to_vec();
    env.push(arg);
    eval(arena, &env, closure.body)
}

/// Convert a value back to a term (for error reporting and definitional equality).
///
/// `depth` is the current De Bruijn level (number of locally-bound variables in scope).
pub fn quote<'a>(arena: &'a Bump, depth: Lvl, val: &Value<'a>) -> &'a Term<'a> {
    match val {
        Value::Rigid(lvl) => {
            let ix = lvl.ix_at_depth(depth);
            arena.alloc(Term::Var(ix))
        }
        Value::Global(name) => arena.alloc(Term::Global(*name)),
        Value::Prim(p) => arena.alloc(Term::Prim(*p)),
        Value::Lit(n, it) => arena.alloc(Term::Lit(*n, *it)),
        Value::U(phase) => match phase {
            Phase::Meta => &Term::TYPE,
            Phase::Object => &Term::VM_TYPE,
        },
        Value::App(f, args) => {
            let qf = quote(arena, depth, f);
            let qargs: Vec<&'a Term<'a>> = args
                .iter()
                .map(|a| quote(arena, depth, a) as &'a _)
                .collect();
            arena.alloc(Term::new_app(qf, arena.alloc_slice_fill_iter(qargs)))
        }
        Value::Lam(vlam) => {
            // Apply the closure to a fresh rigid variable, then quote the result.
            let fresh = Value::Rigid(depth);
            let body_val = inst(arena, &vlam.closure, fresh);
            let body_term = quote(arena, depth.succ(), &body_val);
            let param_ty_term = quote(arena, depth, vlam.param_ty);
            let params = arena.alloc_slice_fill_iter([(vlam.name, param_ty_term as &'a _)]);
            arena.alloc(Term::Lam(Lam {
                params,
                body: body_term,
            }))
        }
        Value::Pi(vpi) => {
            let fresh = Value::Rigid(depth);
            let body_val = inst(arena, &vpi.closure, fresh);
            let body_term = quote(arena, depth.succ(), &body_val);
            let domain_term = quote(arena, depth, vpi.domain);
            let params = arena.alloc_slice_fill_iter([(vpi.name, domain_term as &'a _)]);
            arena.alloc(Term::Pi(Pi {
                params,
                body_ty: body_term,
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
    }
}

/// Definitional equality: quote both values and compare structurally.
pub fn val_eq<'a>(arena: &'a Bump, depth: Lvl, a: &Value<'a>, b: &Value<'a>) -> bool {
    let ta = quote(arena, depth, a);
    let tb = quote(arena, depth, b);
    super::alpha_eq::alpha_eq(ta, tb)
}

/// Evaluate a term in the empty environment.
pub fn eval_closed<'a>(arena: &'a Bump, term: &'a Term<'a>) -> Value<'a> {
    eval(arena, &[], term)
}

/// Extract the Phase from a Value that represents a universe (Type or `VmType`),
/// if it is indeed a type universe.
pub const fn value_phase(val: &Value<'_>) -> Option<Phase> {
    match val {
        Value::Prim(Prim::IntTy(it)) => Some(it.phase),
        Value::Prim(Prim::U(_)) | Value::Lift(_) | Value::Pi(_) | Value::U(_) => Some(Phase::Meta),
        _ => None,
    }
}
