use std::collections::HashMap;

use anyhow::{Result, anyhow, ensure};
use bumpalo::Bump;

use crate::core::{
    Arm, Function, IntType, IntWidth, Ix, Lam, Lvl, Name, Pat, Pi, Prim, Program, Term,
};
use crate::parser::ast::Phase;

// ── Value types ───────────────────────────────────────────────────────────────

/// A value produced by meta-level evaluation.
///
/// Two lifetime parameters:
/// - `'out`: lifetime of the output arena (for `Code` values that appear in the result).
/// - `'eval`: lifetime of the evaluation phase — covers both the input program data (`'core`)
///   and any temporary terms allocated in the local eval arena.  Since `Term` is covariant
///   in its lifetime, `'core` data can be coerced to `'eval` at call sites.
#[derive(Clone, Debug)]
enum MetaVal<'out, 'eval> {
    /// A concrete integer value computed at meta (compile) time.
    Lit(u64),
    /// Quoted object-level code, tagged with the output depth at creation time.
    ///
    /// The embedded term's `Var(Ix(i))` nodes are valid relative to `depth` object bindings in
    /// scope.  When the code value is later spliced into a deeper context, free variable indices
    /// must be shifted by `(current_depth - depth)` before the term can be used.
    Code {
        term: &'out Term<'out>,
        depth: usize,
    },
    /// A type term passed as a type argument (dependent types: types are values).
    /// The type term itself is not inspected during evaluation.
    Ty,
    /// A closure: a lambda body captured with its environment.
    Closure {
        body: &'eval Term<'eval>,
        env: Vec<Binding<'out, 'eval>>,
        obj_next: Lvl,
    },
}

// ── Environment ───────────────────────────────────────────────────────────────

/// A binding stored in the evaluation environment, indexed by De Bruijn level.
#[derive(Clone, Debug)]
enum Binding<'out, 'eval> {
    /// A meta-level variable bound to a concrete `MetaVal`.
    Meta(MetaVal<'out, 'eval>),
    /// An object-level variable.
    Obj(Lvl),
}

/// Evaluation environment: a stack of bindings indexed by De Bruijn index.
///
/// Bindings are stored oldest-first. `Var(Ix(i))` refers to
/// `bindings[bindings.len() - 1 - i]` — the `i`-th binding from the end.
#[derive(Debug)]
struct Env<'out, 'eval> {
    bindings: Vec<Binding<'out, 'eval>>,
    obj_next: Lvl,
}

impl<'out, 'eval> Env<'out, 'eval> {
    const fn new(obj_next: Lvl) -> Self {
        Env {
            bindings: Vec::new(),
            obj_next,
        }
    }

    /// Look up the binding for `Var(Ix(ix))`.
    fn get_ix(&self, ix: Ix) -> &Binding<'out, 'eval> {
        let i = self
            .bindings
            .len()
            .checked_sub(1 + ix.0)
            .expect("De Bruijn index out of environment bounds");
        self.bindings
            .get(i)
            .expect("De Bruijn index out of environment bounds")
    }

    /// Push an object-level binding.
    fn push_obj(&mut self) {
        let lvl = self.obj_next;
        self.obj_next = lvl.succ();
        self.bindings.push(Binding::Obj(lvl));
    }

    /// Push a meta-level binding bound to the given value.
    fn push_meta(&mut self, val: MetaVal<'out, 'eval>) {
        self.bindings.push(Binding::Meta(val));
    }

    /// Pop the last binding.
    fn pop(&mut self) {
        match self.bindings.pop().expect("pop on empty environment") {
            Binding::Obj(_) => {
                self.obj_next = Lvl::new(
                    self.obj_next
                        .0
                        .checked_sub(1)
                        .expect("obj_next underflow on pop"),
                );
            }
            Binding::Meta(_) => {}
        }
    }
}

// ── Globals table ─────────────────────────────────────────────────────────────

/// Everything the evaluator needs to know about a top-level function.
struct GlobalDef<'a> {
    ty: &'a Pi<'a>,
    body: &'a Term<'a>,
}

type Globals<'a> = HashMap<&'a Name, GlobalDef<'a>>;

// ── Meta-level evaluator ──────────────────────────────────────────────────────

/// Evaluate a meta-level `term` to a `MetaVal`.
fn eval_meta<'out, 'eval>(
    arena: &'out Bump,
    eval_arena: &'eval Bump,
    globals: &Globals<'eval>,
    env: &mut Env<'out, 'eval>,
    term: &'eval Term<'eval>,
) -> Result<MetaVal<'out, 'eval>> {
    match term {
        // ── Variable ─────────────────────────────────────────────────────────
        Term::Var(ix) => match env.get_ix(*ix) {
            Binding::Meta(v) => Ok(v.clone()),
            Binding::Obj(_) => unreachable!(
                "object variable at index {} referenced in meta context (typechecker invariant)",
                ix.0
            ),
        },

        // ── Literal ──────────────────────────────────────────────────────────
        Term::Lit(n, _) => Ok(MetaVal::Lit(*n)),

        // ── Global reference ─────────────────────────────────────────────────
        Term::Global(name) => {
            let def = globals
                .get(name)
                .unwrap_or_else(|| panic!("unknown global `{name}` during staging"));
            let pi = def.ty;
            if pi.params.is_empty() {
                // Zero-param global: evaluate the body immediately in a fresh env.
                let mut callee_env = Env::new(env.obj_next);
                eval_meta(arena, eval_arena, globals, &mut callee_env, def.body)
            } else {
                // Multi-param global: produce a closure.
                Ok(global_to_closure(eval_arena, def, env.obj_next))
            }
        }

        // ── Lambda ───────────────────────────────────────────────────────────
        Term::Lam(lam) => {
            // For a zero-param lambda (thunk), wrap in a Closure whose body IS the
            // lambda body; force_thunk evaluates it when applied to zero args.
            // For a multi-param lambda, wrap params[1..] in a synthetic Lam so that
            // apply_closure can peel one param at a time.
            let body = match lam.params {
                [] | [_] => lam.body,
                [_, rest @ ..] => eval_arena.alloc(Term::Lam(Lam {
                    params: rest,
                    body: lam.body,
                })),
            };
            Ok(MetaVal::Closure {
                body,
                env: env.bindings.clone(),
                obj_next: env.obj_next,
            })
        }

        // ── Application ──────────────────────────────────────────────────────
        Term::App(app) => match app.func {
            Term::Prim(prim) => eval_meta_prim(arena, eval_arena, globals, env, *prim, app.args),
            _ => {
                let mut val = eval_meta(arena, eval_arena, globals, env, app.func)?;
                if app.args.is_empty() {
                    // Zero-arg call: force the thunk closure.
                    val = force_thunk(arena, eval_arena, globals, val)?;
                } else {
                    for arg in app.args {
                        let arg_val = eval_meta(arena, eval_arena, globals, env, arg)?;
                        val = apply_closure(arena, eval_arena, globals, val, arg_val)?;
                    }
                }
                Ok(val)
            }
        },

        // ── Quote: #(t) ──────────────────────────────────────────────────────
        Term::Quote(inner) => {
            let obj_term = unstage_obj(arena, eval_arena, globals, env, inner)?;
            Ok(MetaVal::Code {
                term: obj_term,
                depth: env.obj_next.0,
            })
        }

        // ── Let binding ──────────────────────────────────────────────────────
        Term::Let(let_) => {
            let val = eval_meta(arena, eval_arena, globals, env, let_.expr)?;
            env.push_meta(val);
            let result = eval_meta(arena, eval_arena, globals, env, let_.body);
            env.pop();
            result
        }

        // ── Match ────────────────────────────────────────────────────────────
        Term::Match(match_) => {
            let scrut_val = eval_meta(arena, eval_arena, globals, env, match_.scrutinee)?;
            let n = match scrut_val {
                MetaVal::Lit(n) => n,
                MetaVal::Code { .. } | MetaVal::Ty | MetaVal::Closure { .. } => unreachable!(
                    "cannot match on non-integer at meta level (typechecker invariant)"
                ),
            };
            eval_meta_match(arena, eval_arena, globals, env, n, match_.arms)
        }

        // ── Unreachable in well-typed meta terms ─────────────────────────────
        Term::Splice(_) => unreachable!("Splice in meta context (typechecker invariant)"),
        // Type-level terms evaluate to themselves when passed as type arguments
        // in a dependently-typed function call (e.g. `id(u64, x)` passes `u64 : Type`).
        Term::Lift(_) | Term::Prim(_) | Term::Pi(_) => Ok(MetaVal::Ty),
    }
}

/// Convert a global function definition into a closure value.
///
/// For a multi-parameter function, we build nested closures.  E.g., `fn f(x, y) = body`
/// becomes a closure whose body is a lambda `|y| body`.  The synthetic `Lam` wrapper nodes
/// are allocated in `eval_arena`, which is local to `unstage_program` and lives for the
/// duration of staging — long enough to outlive any closure values.
fn global_to_closure<'out, 'eval>(
    eval_arena: &'eval Bump,
    def: &GlobalDef<'eval>,
    obj_next: Lvl,
) -> MetaVal<'out, 'eval> {
    // Called only when params is non-empty (zero-param globals are evaluated immediately).
    let pi = def.ty;
    let body = match pi.params {
        [_] | [] => def.body,
        [_, rest @ ..] => eval_arena.alloc(Term::Lam(Lam {
            params: rest,
            body: def.body,
        })),
    };
    MetaVal::Closure {
        body,
        env: Vec::new(),
        obj_next,
    }
}

/// Apply a closure value to an argument value.
fn apply_closure<'out, 'eval>(
    arena: &'out Bump,
    eval_arena: &'eval Bump,
    globals: &Globals<'eval>,
    func_val: MetaVal<'out, 'eval>,
    arg_val: MetaVal<'out, 'eval>,
) -> Result<MetaVal<'out, 'eval>> {
    match func_val {
        MetaVal::Closure {
            body,
            env,
            obj_next,
            ..
        } => {
            let mut callee_env = Env {
                bindings: env,
                obj_next,
            };
            callee_env.push_meta(arg_val);

            eval_meta(arena, eval_arena, globals, &mut callee_env, body)
        }
        MetaVal::Lit(_) | MetaVal::Code { .. } | MetaVal::Ty => {
            unreachable!("applying a non-function value (typechecker invariant)")
        }
    }
}

/// Force a thunk closure: evaluate its body in the captured environment without pushing any arg.
fn force_thunk<'out, 'eval>(
    arena: &'out Bump,
    eval_arena: &'eval Bump,
    globals: &Globals<'eval>,
    val: MetaVal<'out, 'eval>,
) -> Result<MetaVal<'out, 'eval>> {
    match val {
        MetaVal::Closure {
            body,
            env,
            obj_next,
            ..
        } => {
            let mut callee_env = Env {
                bindings: env,
                obj_next,
            };
            eval_meta(arena, eval_arena, globals, &mut callee_env, body)
        }
        // Already-evaluated value (e.g. a zero-param global reduced to Lit/Code).
        // A zero-arg call is a no-op in this case.
        other => Ok(other),
    }
}

/// Evaluate a primitive operation at meta level.
fn eval_meta_prim<'out, 'eval>(
    arena: &'out Bump,
    eval_arena: &'eval Bump,
    globals: &Globals<'eval>,
    env: &mut Env<'out, 'eval>,
    prim: Prim,
    args: &'eval [&'eval Term<'eval>],
) -> Result<MetaVal<'out, 'eval>> {
    let eval_lit = |arena: &'out Bump,
                    eval_arena: &'eval Bump,
                    globals: &Globals<'eval>,
                    env: &mut Env<'out, 'eval>,
                    arg: &'eval Term<'eval>| {
        eval_meta(arena, eval_arena, globals, env, arg).map(|v| match v {
            MetaVal::Lit(n) => n,
            MetaVal::Code { .. } | MetaVal::Ty | MetaVal::Closure { .. } => unreachable!(
                "expected integer meta value for primitive operand (typechecker invariant)"
            ),
        })
    };

    #[expect(clippy::indexing_slicing)]
    match prim {
        // ── Arithmetic ────────────────────────────────────────────────────────
        Prim::Add(IntType { width, .. }) => {
            let a = eval_lit(arena, eval_arena, globals, env, args[0])?;
            let b = eval_lit(arena, eval_arena, globals, env, args[1])?;
            let result = a
                .checked_add(b)
                .filter(|&r| r <= width.max_value())
                .ok_or_else(|| {
                    anyhow!(
                        "arithmetic overflow during staging: \
                         {a} + {b} = {} exceeds maximum value of {width} ({})",
                        a.wrapping_add(b),
                        width.max_value()
                    )
                })?;
            Ok(MetaVal::Lit(result))
        }
        Prim::Sub(IntType { width, .. }) => {
            let a = eval_lit(arena, eval_arena, globals, env, args[0])?;
            let b = eval_lit(arena, eval_arena, globals, env, args[1])?;
            let result = a.checked_sub(b).ok_or_else(|| {
                anyhow!(
                    "arithmetic overflow during staging: \
                     {a} - {b} underflows {width}"
                )
            })?;
            Ok(MetaVal::Lit(result))
        }
        Prim::Mul(IntType { width, .. }) => {
            let a = eval_lit(arena, eval_arena, globals, env, args[0])?;
            let b = eval_lit(arena, eval_arena, globals, env, args[1])?;
            let result = a
                .checked_mul(b)
                .filter(|&r| r <= width.max_value())
                .ok_or_else(|| {
                    anyhow!(
                        "arithmetic overflow during staging: \
                         {a} * {b} = {} exceeds maximum value of {width} ({})",
                        a.wrapping_mul(b),
                        width.max_value()
                    )
                })?;
            Ok(MetaVal::Lit(result))
        }
        Prim::Div(_) => {
            let a = eval_lit(arena, eval_arena, globals, env, args[0])?;
            let b = eval_lit(arena, eval_arena, globals, env, args[1])?;
            ensure!(b != 0, "division by zero during staging");
            Ok(MetaVal::Lit(a / b))
        }

        // ── Bitwise ───────────────────────────────────────────────────────────
        Prim::BitAnd(_) => {
            let a = eval_lit(arena, eval_arena, globals, env, args[0])?;
            let b = eval_lit(arena, eval_arena, globals, env, args[1])?;
            Ok(MetaVal::Lit(a & b))
        }
        Prim::BitOr(_) => {
            let a = eval_lit(arena, eval_arena, globals, env, args[0])?;
            let b = eval_lit(arena, eval_arena, globals, env, args[1])?;
            Ok(MetaVal::Lit(a | b))
        }
        Prim::BitNot(IntType { width, .. }) => {
            let a = eval_lit(arena, eval_arena, globals, env, args[0])?;
            Ok(MetaVal::Lit(mask_to_width(width, !a)))
        }

        // ── Comparison ────────────────────────────────────────────────────────
        Prim::Eq(_) => {
            let a = eval_lit(arena, eval_arena, globals, env, args[0])?;
            let b = eval_lit(arena, eval_arena, globals, env, args[1])?;
            Ok(MetaVal::Lit(u64::from(a == b)))
        }
        Prim::Ne(_) => {
            let a = eval_lit(arena, eval_arena, globals, env, args[0])?;
            let b = eval_lit(arena, eval_arena, globals, env, args[1])?;
            Ok(MetaVal::Lit(u64::from(a != b)))
        }
        Prim::Lt(_) => {
            let a = eval_lit(arena, eval_arena, globals, env, args[0])?;
            let b = eval_lit(arena, eval_arena, globals, env, args[1])?;
            Ok(MetaVal::Lit(u64::from(a < b)))
        }
        Prim::Gt(_) => {
            let a = eval_lit(arena, eval_arena, globals, env, args[0])?;
            let b = eval_lit(arena, eval_arena, globals, env, args[1])?;
            Ok(MetaVal::Lit(u64::from(a > b)))
        }
        Prim::Le(_) => {
            let a = eval_lit(arena, eval_arena, globals, env, args[0])?;
            let b = eval_lit(arena, eval_arena, globals, env, args[1])?;
            Ok(MetaVal::Lit(u64::from(a <= b)))
        }
        Prim::Ge(_) => {
            let a = eval_lit(arena, eval_arena, globals, env, args[0])?;
            let b = eval_lit(arena, eval_arena, globals, env, args[1])?;
            Ok(MetaVal::Lit(u64::from(a >= b)))
        }

        // ── Embed: meta integer → object code ─────────────────────────────────
        Prim::Embed(width) => {
            let n = eval_lit(arena, eval_arena, globals, env, args[0])?;
            let phase = Phase::Object;
            let lit_term = arena.alloc(Term::Lit(n, IntType { width, phase }));
            Ok(MetaVal::Code {
                term: lit_term,
                depth: env.obj_next.0,
            })
        }

        // ── Type-level prims are unreachable ──────────────────────────────────
        Prim::IntTy(_) | Prim::U(_) => {
            unreachable!("type-level primitive in evaluation position (typechecker invariant)")
        }
    }
}

/// Mask `val` to the bit-width of `width`.
const fn mask_to_width(width: IntWidth, val: u64) -> u64 {
    match width {
        IntWidth::U0 => 0,
        IntWidth::U1 => val & 0x1,
        IntWidth::U8 => val & 0xff,
        IntWidth::U16 => val & 0xffff,
        IntWidth::U32 => val & 0xffff_ffff,
        IntWidth::U64 => val,
    }
}

/// Evaluate a meta-level `match` expression.
fn eval_meta_match<'out, 'eval>(
    arena: &'out Bump,
    eval_arena: &'eval Bump,
    globals: &Globals<'eval>,
    env: &mut Env<'out, 'eval>,
    n: u64,
    arms: &'eval [Arm<'eval>],
) -> Result<MetaVal<'out, 'eval>> {
    for arm in arms {
        match &arm.pat {
            Pat::Lit(m) => {
                if n == *m {
                    return eval_meta(arena, eval_arena, globals, env, arm.body);
                }
            }
            Pat::Bind(_) | Pat::Wildcard => {
                env.push_meta(MetaVal::Lit(n));
                let result = eval_meta(arena, eval_arena, globals, env, arm.body);
                env.pop();
                return result;
            }
        }
    }
    Err(anyhow!(
        "non-exhaustive match during staging (scrutinee = {n})"
    ))
}

// ── Index shifting ────────────────────────────────────────────────────────────

/// Shift all free (>= `cutoff`) De Bruijn indices in `term` upward by `shift`.
///
/// Used when splicing a `Code` value that was created at a shallower output depth into a deeper
/// context: every free variable index must increase by the depth difference.
///
/// TODO(#29): This shifting step would be eliminated if object-level code used NbE with
/// closures and De Bruijn levels (as in the Kovács reference impl) instead of raw terms.
fn shift_free_ix<'out>(
    arena: &'out Bump,
    term: &'out Term<'out>,
    shift: usize,
    cutoff: usize,
) -> &'out Term<'out> {
    if shift == 0 {
        return term;
    }
    match term {
        Term::Var(Ix(i)) => {
            if *i >= cutoff {
                arena.alloc(Term::Var(Ix(i + shift)))
            } else {
                term
            }
        }
        Term::Prim(_) | Term::Lit(_, _) | Term::Global(_) => term,
        Term::App(app) => {
            let new_func = shift_free_ix(arena, app.func, shift, cutoff);
            let new_args = arena.alloc_slice_fill_iter(
                app.args
                    .iter()
                    .map(|a| shift_free_ix(arena, a, shift, cutoff)),
            );
            arena.alloc(Term::new_app(new_func, new_args))
        }
        Term::Lam(lam) => {
            let mut c = cutoff;
            let new_params = arena.alloc_slice_fill_iter(lam.params.iter().map(|&(name, ty)| {
                let new_ty = shift_free_ix(arena, ty, shift, c);
                c += 1;
                (name, new_ty as &'out Term<'out>)
            }));
            let new_body = shift_free_ix(arena, lam.body, shift, c);
            arena.alloc(Term::Lam(Lam {
                params: new_params,
                body: new_body,
            }))
        }
        Term::Pi(pi) => {
            let mut c = cutoff;
            let new_params = arena.alloc_slice_fill_iter(pi.params.iter().map(|&(name, ty)| {
                let new_ty = shift_free_ix(arena, ty, shift, c);
                c += 1;
                (name, new_ty as &'out Term<'out>)
            }));
            let new_body_ty = shift_free_ix(arena, pi.body_ty, shift, c);
            arena.alloc(Term::Pi(Pi {
                params: new_params,
                body_ty: new_body_ty,
                phase: pi.phase,
            }))
        }
        Term::Lift(inner) => arena.alloc(Term::Lift(shift_free_ix(arena, inner, shift, cutoff))),
        Term::Quote(inner) => arena.alloc(Term::Quote(shift_free_ix(arena, inner, shift, cutoff))),
        Term::Splice(inner) => {
            arena.alloc(Term::Splice(shift_free_ix(arena, inner, shift, cutoff)))
        }
        Term::Let(let_) => {
            let new_ty = shift_free_ix(arena, let_.ty, shift, cutoff);
            let new_expr = shift_free_ix(arena, let_.expr, shift, cutoff);
            let new_body = shift_free_ix(arena, let_.body, shift, cutoff + 1);
            arena.alloc(Term::new_let(let_.name, new_ty, new_expr, new_body))
        }
        Term::Match(match_) => {
            let new_scrutinee = shift_free_ix(arena, match_.scrutinee, shift, cutoff);
            let new_arms = arena.alloc_slice_fill_iter(match_.arms.iter().map(|arm| {
                let arm_cutoff = cutoff + usize::from(arm.pat.bound_name().is_some());
                Arm {
                    pat: arm.pat.clone(),
                    body: shift_free_ix(arena, arm.body, shift, arm_cutoff),
                }
            }));
            arena.alloc(Term::new_match(new_scrutinee, new_arms))
        }
    }
}

// ── Object-level unstager ─────────────────────────────────────────────────────

/// Unstage an object-level `term`, eliminating all `Splice` nodes.
fn unstage_obj<'out, 'eval>(
    arena: &'out Bump,
    eval_arena: &'eval Bump,
    globals: &Globals<'eval>,
    env: &mut Env<'out, 'eval>,
    term: &'eval Term<'eval>,
) -> Result<&'out Term<'out>> {
    match term {
        // ── Variable ─────────────────────────────────────────────────────────
        Term::Var(ix) => match env.get_ix(*ix) {
            Binding::Obj(out_lvl) => {
                // Convert output level → De Bruijn index relative to current output depth.
                let out_ix = Ix(env.obj_next.0 - out_lvl.0 - 1);
                Ok(arena.alloc(Term::Var(out_ix)))
            }
            Binding::Meta(MetaVal::Code { term, depth }) => {
                Ok(shift_free_ix(arena, term, env.obj_next.0 - depth, 0))
            }
            Binding::Meta(MetaVal::Lit(_)) => unreachable!(
                "integer meta variable at index {} referenced in object context \
                 (typechecker invariant)",
                ix.0
            ),
            Binding::Meta(MetaVal::Closure { .. }) => unreachable!(
                "closure meta variable at index {} referenced in object context \
                 (typechecker invariant)",
                ix.0
            ),
            Binding::Meta(MetaVal::Ty) => unreachable!(
                "type meta variable at index {} referenced in object context \
                 (typechecker invariant)",
                ix.0
            ),
        },

        // ── Literal ──────────────────────────────────────────────────────────
        Term::Lit(n, it) => Ok(arena.alloc(Term::Lit(*n, *it))),

        // ── Primitive ────────────────────────────────────────────────────────
        Term::Prim(p) => Ok(arena.alloc(Term::Prim(*p))),

        // ── Global reference (in object terms, e.g. object-level function call) ──
        Term::Global(name) => {
            Ok(arena.alloc(Term::Global(Name::new(arena.alloc_str(name.as_str())))))
        }

        // ── App ───────────────────────────────────────────────────────────────
        Term::App(app) => {
            let staged_func = unstage_obj(arena, eval_arena, globals, env, app.func)?;
            let staged_args: &'out [&'out Term<'out>] = arena.alloc_slice_try_fill_iter(
                app.args
                    .iter()
                    .map(|arg| unstage_obj(arena, eval_arena, globals, env, arg)),
            )?;
            Ok(arena.alloc(Term::new_app(staged_func, staged_args)))
        }

        // ── Splice: $(t) — the key staging step ──────────────────────────────
        Term::Splice(inner) => {
            let meta_val = eval_meta(arena, eval_arena, globals, env, inner)?;
            match meta_val {
                MetaVal::Code { term, depth } => {
                    Ok(shift_free_ix(arena, term, env.obj_next.0 - depth, 0))
                }
                MetaVal::Lit(_) | MetaVal::Ty | MetaVal::Closure { .. } => {
                    unreachable!("splice evaluated to non-code value (typechecker invariant)")
                }
            }
        }

        // ── Let binding ──────────────────────────────────────────────────────
        Term::Let(let_) => {
            let staged_ty = unstage_obj(arena, eval_arena, globals, env, let_.ty)?;
            let staged_expr = unstage_obj(arena, eval_arena, globals, env, let_.expr)?;
            env.push_obj();
            let staged_body = unstage_obj(arena, eval_arena, globals, env, let_.body);
            env.pop();
            Ok(arena.alloc(Term::new_let(
                Name::new(arena.alloc_str(let_.name.as_str())),
                staged_ty,
                staged_expr,
                staged_body?,
            )))
        }

        // ── Match ────────────────────────────────────────────────────────────
        Term::Match(match_) => {
            let staged_scrutinee = unstage_obj(arena, eval_arena, globals, env, match_.scrutinee)?;
            let staged_arms: &'out [Arm<'out>] =
                arena.alloc_slice_try_fill_iter(match_.arms.iter().map(|arm| -> Result<_> {
                    let staged_pat = match &arm.pat {
                        Pat::Lit(n) => Pat::Lit(*n),
                        Pat::Bind(name) => Pat::Bind(Name::new(arena.alloc_str(name.as_str()))),
                        Pat::Wildcard => Pat::Wildcard,
                    };
                    let has_binding = arm.pat.bound_name().is_some();
                    if has_binding {
                        env.push_obj();
                    }
                    let staged_body = unstage_obj(arena, eval_arena, globals, env, arm.body);
                    if has_binding {
                        env.pop();
                    }
                    Ok(Arm {
                        pat: staged_pat,
                        body: staged_body?,
                    })
                }))?;
            Ok(arena.alloc(Term::new_match(staged_scrutinee, staged_arms)))
        }

        // ── Unreachable in well-typed object terms ───────────────────────────
        Term::Quote(_) => unreachable!("Quote in object context (typechecker invariant)"),
        Term::Lift(_) | Term::Pi(_) | Term::Lam(_) => {
            unreachable!("meta-only term in object context (typechecker invariant)")
        }
    }
}

// ── Public entry point ────────────────────────────────────────────────────────

/// Unstage an elaborated program.
///
/// - `arena`: output arena; the returned `Program<'out>` is allocated here.
/// - `program`: input core program; may be dropped once this function returns.
pub fn unstage_program<'out, 'core>(
    arena: &'out Bump,
    program: &'core Program<'core>,
) -> Result<Program<'out>> {
    // A temporary arena for intermediate values (synthetic Lam wrappers for closures, etc.)
    // that exist only during evaluation and must not appear in the output.  Its lifetime
    // `'eval` is shorter than `'core`, so `'core` data is coercible to `'eval` via the
    // covariance of `Term`.
    let eval_arena = Bump::new();

    let globals: Globals<'_> = program
        .functions
        .iter()
        .map(|f| {
            (
                f.name,
                GlobalDef {
                    ty: f.ty,
                    body: f.body,
                },
            )
        })
        .collect();

    let staged_fns: Vec<Function<'out>> = program
        .functions
        .iter()
        .filter(|f| f.pi().phase == Phase::Object)
        .map(|f| -> Result<_> {
            let pi = f.pi();
            let mut env = Env::new(Lvl::new(0));

            let staged_params = arena.alloc_slice_try_fill_iter(pi.params.iter().map(
                |(n, ty)| -> Result<(&'out Name, &'out Term<'out>)> {
                    let staged_ty = unstage_obj(arena, &eval_arena, &globals, &mut env, ty)?;
                    env.push_obj();
                    Ok((Name::new(arena.alloc_str(n.as_str())), staged_ty))
                },
            ))?;

            let staged_ret_ty = unstage_obj(arena, &eval_arena, &globals, &mut env, pi.body_ty)?;
            let staged_body = unstage_obj(arena, &eval_arena, &globals, &mut env, f.body)?;

            Ok(Function {
                name: Name::new(arena.alloc_str(f.name.as_str())),
                ty: arena.alloc(Pi {
                    params: staged_params,
                    body_ty: staged_ret_ty,
                    phase: Phase::Object,
                }),
                body: staged_body,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    let functions = arena.alloc_slice_fill_iter(staged_fns);
    Ok(Program { functions })
}
