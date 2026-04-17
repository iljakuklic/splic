use std::collections::HashMap;

use anyhow::{Result, anyhow, ensure};
use bumpalo::Bump;

use crate::common::de_bruijn;
use crate::common::env::Env as LevelEnv;
use crate::core::{self, Arm, IntType, IntWidth, Name, Pat, Prim, Program, Term};
use crate::parser::ast::Phase;

// ── Object-level semantic values ──────────────────────────────────────────────

/// A fully-evaluated object-level value using De Bruijn **levels**.
///
/// Variables are represented as absolute levels (not indices), so splicing a
/// value into any depth requires no index shifting.  A final `quote_obj` pass
/// converts levels back to De Bruijn indices.
///
/// No closures are needed here because object-level lambdas and Pi types are
/// forbidden in Splic (the type-checker enforces this); `eval_obj` eagerly
/// evaluates under binders by extending the environment with fresh level variables.
#[derive(Clone, Debug)]
enum ObjVal<'names, 'eval> {
    /// Local variable identified by De Bruijn level (absolute, context-independent).
    Var(de_bruijn::Lvl),
    /// Integer literal.
    Lit(u64, IntType),
    /// Unapplied primitive.
    Prim(Prim),
    /// Global function reference.
    Global(&'names Name),
    /// Application.
    App(&'eval Self, &'eval [Self]),
    /// Let binding.
    Let {
        name: &'names Name,
        ty: &'eval Self,
        expr: &'eval Self,
        body: &'eval Self,
    },
    /// Pattern match.
    Match {
        scrutinee: &'eval Self,
        arms: &'eval [ObjArm<'names, 'eval>],
    },
}

#[derive(Clone, Debug)]
struct ObjArm<'names, 'eval> {
    pat: Pat<'names>,
    body: &'eval ObjVal<'names, 'eval>,
}

// ── Meta-level values ─────────────────────────────────────────────────────────

/// A value produced by meta-level evaluation.
///
/// The lifetime `'eval` covers both the input program data (`'core`) and any
/// temporary data allocated in the local eval arena.  Since `Term` is covariant
/// in its lifetime, `'core` data can be coerced to `'eval` at call sites.
///
/// `ObjVal` trees are allocated in the eval arena (same lifetime `'eval`), so
/// they are automatically discarded after staging; only the final `Term` output
/// from `quote_obj` survives in the output arena.
#[derive(Clone, Debug)]
enum MetaVal<'names, 'eval> {
    /// A concrete integer value computed at meta (compile) time.
    Lit(u64),
    /// Quoted object-level code as a semantic value.
    ///
    /// Uses De Bruijn levels internally, so it can be spliced into any depth
    /// without adjustment.  Allocated in the eval arena.
    Quote(&'eval ObjVal<'names, 'eval>),
    /// A type term passed as a type argument (dependent types: types are values).
    /// The type term itself is not inspected during evaluation.
    Ty,
    /// A closure: a lambda body captured with its environment.
    Closure {
        body: &'eval Term<'names, 'eval>,
        arity: usize,
        env: LevelEnv<Binding<'names, 'eval>>,
        obj_depth: de_bruijn::Depth,
    },
}

// ── Environment ───────────────────────────────────────────────────────────────

/// A binding stored in the evaluation environment, indexed by De Bruijn level.
#[derive(Clone, Debug)]
enum Binding<'names, 'eval> {
    /// A meta-level variable bound to a concrete `MetaVal`.
    Meta(MetaVal<'names, 'eval>),
    /// An object-level variable.
    Obj(de_bruijn::Lvl),
}

/// Evaluation environment: a stack of bindings indexed by De Bruijn index.
///
/// Bindings are stored oldest-first. `Var(Ix(i))` refers to
/// `bindings[bindings.len() - 1 - i]` — the `i`-th binding from the end.
#[derive(Debug)]
struct Env<'names, 'eval> {
    bindings: LevelEnv<Binding<'names, 'eval>>,
    obj_depth: de_bruijn::Depth,
}

impl<'names, 'eval> Env<'names, 'eval> {
    const fn new(obj_depth: de_bruijn::Depth) -> Self {
        Env {
            bindings: LevelEnv::new(),
            obj_depth,
        }
    }

    /// Look up the binding for `Var(Ix(ix))`.
    fn get_ix(&self, ix: de_bruijn::Ix) -> &Binding<'names, 'eval> {
        &self.bindings[ix]
    }

    /// Push an object-level binding.
    fn push_obj(&mut self) {
        let lvl = self.obj_depth.as_lvl();
        self.obj_depth = self.obj_depth.succ();
        self.bindings.push(Binding::Obj(lvl));
    }

    /// Push a meta-level binding bound to the given value.
    fn push_meta(&mut self, val: MetaVal<'names, 'eval>) {
        self.bindings.push(Binding::Meta(val));
    }

    /// Pop the last binding.
    fn pop(&mut self) {
        match self.bindings.pop() {
            Binding::Obj(_) => {
                self.obj_depth = self.obj_depth.pred();
            }
            Binding::Meta(_) => {}
        }
    }
}

// ── Globals table ─────────────────────────────────────────────────────────────

type Globals<'names, 'a> = HashMap<&'names Name, &'a Term<'names, 'a>>;

// ── Meta-level evaluator ──────────────────────────────────────────────────────

/// Evaluate a meta-level `term` to a `MetaVal`.
fn eval_meta<'names, 'eval>(
    eval_arena: &'eval Bump,
    globals: &Globals<'names, 'eval>,
    env: &mut Env<'names, 'eval>,
    term: &'eval Term<'names, 'eval>,
) -> Result<MetaVal<'names, 'eval>> {
    match term {
        // ── Variable ─────────────────────────────────────────────────────────
        Term::Var(ix) => match env.get_ix(*ix) {
            Binding::Meta(v) => Ok(v.clone()),
            Binding::Obj(_) => unreachable!(
                "object variable at index {:?} referenced in meta context (typechecker invariant)",
                ix
            ),
        },

        // ── Literal ──────────────────────────────────────────────────────────
        Term::Lit(n, _) => Ok(MetaVal::Lit(*n)),

        // ── Global reference ─────────────────────────────────────────────────
        Term::Global(name) => {
            let body = globals
                .get(name)
                .copied()
                .unwrap_or_else(|| panic!("unknown global `{name}` during staging"));
            // Evaluate the body in a fresh env at the current obj depth.
            // For function defs (Lam body) this produces a Closure; for
            // constants it produces the constant's value directly.
            let mut global_env = Env::new(env.obj_depth);
            eval_meta(eval_arena, globals, &mut global_env, body)
        }

        // ── Lambda ───────────────────────────────────────────────────────────
        Term::Lam(lam) => Ok(MetaVal::Closure {
            body: lam.body,
            arity: lam.params.len(),
            env: env.bindings.clone(),
            obj_depth: env.obj_depth,
        }),

        // ── Application ──────────────────────────────────────────────────────
        Term::App(app) => match app.func {
            Term::Prim(prim) => eval_meta_prim(eval_arena, globals, env, *prim, app.args),
            _ => {
                let func_val = eval_meta(eval_arena, globals, env, app.func)?;
                let arg_vals: Vec<MetaVal<'names, 'eval>> = app
                    .args
                    .iter()
                    .map(|a| eval_meta(eval_arena, globals, env, a))
                    .collect::<Result<_>>()?;
                apply_closure_n(eval_arena, globals, func_val, &arg_vals)
            }
        },

        // ── Quote: #(t) ──────────────────────────────────────────────────────
        Term::Quote(inner) => {
            let obj_val = eval_obj(eval_arena, globals, env, inner)?;
            Ok(MetaVal::Quote(obj_val))
        }

        // ── Let binding ──────────────────────────────────────────────────────
        Term::Let(let_) => {
            let val = eval_meta(eval_arena, globals, env, let_.expr)?;
            env.push_meta(val);
            let result = eval_meta(eval_arena, globals, env, let_.body);
            env.pop();
            result
        }

        // ── Match ────────────────────────────────────────────────────────────
        Term::Match(match_) => {
            let scrut_val = eval_meta(eval_arena, globals, env, match_.scrutinee)?;
            let n = match scrut_val {
                MetaVal::Lit(n) => n,
                MetaVal::Quote(_) | MetaVal::Ty | MetaVal::Closure { .. } => unreachable!(
                    "cannot match on non-integer at meta level (typechecker invariant)"
                ),
            };
            eval_meta_match(eval_arena, globals, env, n, match_.arms)
        }

        // ── Unreachable in well-typed meta terms ─────────────────────────────
        Term::Splice(_) => unreachable!("Splice in meta context (typechecker invariant)"),
        // Type-level terms evaluate to themselves when passed as type arguments
        // in a dependently-typed function call (e.g. `id(u64, x)` passes `u64 : Type`).
        Term::Lift(_) | Term::Prim(_) | Term::Pi(_) => Ok(MetaVal::Ty),
    }
}

/// Apply a closure value to N arguments simultaneously.
fn apply_closure_n<'names, 'eval>(
    eval_arena: &'eval Bump,
    globals: &Globals<'names, 'eval>,
    func_val: MetaVal<'names, 'eval>,
    args: &[MetaVal<'names, 'eval>],
) -> Result<MetaVal<'names, 'eval>> {
    match func_val {
        MetaVal::Closure {
            body,
            arity,
            env,
            obj_depth,
        } => {
            debug_assert_eq!(args.len(), arity, "arity mismatch in apply_closure_n");
            let mut callee_env = Env {
                bindings: env,
                obj_depth,
            };
            for arg in args {
                callee_env.push_meta(arg.clone());
            }
            eval_meta(eval_arena, globals, &mut callee_env, body)
        }
        MetaVal::Lit(_) | MetaVal::Quote(_) | MetaVal::Ty => {
            unreachable!("applying a non-function value (typechecker invariant)")
        }
    }
}

fn eval_lit<'names, 'eval>(
    eval_arena: &'eval Bump,
    globals: &Globals<'names, 'eval>,
    env: &mut Env<'names, 'eval>,
    arg: &'eval Term<'names, 'eval>,
) -> Result<u64> {
    eval_meta(eval_arena, globals, env, arg).map(|v| match v {
        MetaVal::Lit(n) => n,
        MetaVal::Quote(_) | MetaVal::Ty | MetaVal::Closure { .. } => unreachable!(
            "expected integer meta value for primitive operand (typechecker invariant)"
        ),
    })
}

fn eval_bin_args<'names, 'eval>(
    eval_arena: &'eval Bump,
    globals: &Globals<'names, 'eval>,
    env: &mut Env<'names, 'eval>,
    args: &'eval [&'eval Term<'names, 'eval>],
) -> Result<(u64, u64)> {
    let [lhs, rhs] = args else {
        panic!("binary primitive requires exactly 2 arguments (typechecker invariant)")
    };
    let a = eval_lit(eval_arena, globals, env, lhs)?;
    let b = eval_lit(eval_arena, globals, env, rhs)?;
    Ok((a, b))
}

/// Evaluate a primitive operation at meta level.
fn eval_meta_prim<'names, 'eval>(
    eval_arena: &'eval Bump,
    globals: &Globals<'names, 'eval>,
    env: &mut Env<'names, 'eval>,
    prim: Prim,
    args: &'eval [&'eval Term<'names, 'eval>],
) -> Result<MetaVal<'names, 'eval>> {
    match prim {
        // ── Arithmetic ────────────────────────────────────────────────────────
        Prim::Add(IntType { width, .. }) => {
            let (a, b) = eval_bin_args(eval_arena, globals, env, args)?;
            Ok(MetaVal::Lit(mask_to_width(width, a.wrapping_add(b))))
        }
        Prim::Sub(IntType { width, .. }) => {
            let (a, b) = eval_bin_args(eval_arena, globals, env, args)?;
            Ok(MetaVal::Lit(mask_to_width(width, a.wrapping_sub(b))))
        }
        Prim::Mul(IntType { width, .. }) => {
            let (a, b) = eval_bin_args(eval_arena, globals, env, args)?;
            Ok(MetaVal::Lit(mask_to_width(width, a.wrapping_mul(b))))
        }
        Prim::Div(_) => {
            let (a, b) = eval_bin_args(eval_arena, globals, env, args)?;
            ensure!(b != 0, "division by zero during staging");
            Ok(MetaVal::Lit(a / b))
        }

        // ── Bitwise ───────────────────────────────────────────────────────────
        Prim::BitAnd(_) => {
            let (a, b) = eval_bin_args(eval_arena, globals, env, args)?;
            Ok(MetaVal::Lit(a & b))
        }
        Prim::BitOr(_) => {
            let (a, b) = eval_bin_args(eval_arena, globals, env, args)?;
            Ok(MetaVal::Lit(a | b))
        }
        Prim::BitNot(IntType { width, .. }) => {
            let [arg] = args else {
                panic!("unary primitive requires exactly 1 argument (typechecker invariant)")
            };
            let a = eval_lit(eval_arena, globals, env, arg)?;
            Ok(MetaVal::Lit(mask_to_width(width, !a)))
        }

        // ── Comparison ────────────────────────────────────────────────────────
        Prim::Eq(_) => {
            let (a, b) = eval_bin_args(eval_arena, globals, env, args)?;
            Ok(MetaVal::Lit(u64::from(a == b)))
        }
        Prim::Ne(_) => {
            let (a, b) = eval_bin_args(eval_arena, globals, env, args)?;
            Ok(MetaVal::Lit(u64::from(a != b)))
        }
        Prim::Lt(_) => {
            let (a, b) = eval_bin_args(eval_arena, globals, env, args)?;
            Ok(MetaVal::Lit(u64::from(a < b)))
        }
        Prim::Gt(_) => {
            let (a, b) = eval_bin_args(eval_arena, globals, env, args)?;
            Ok(MetaVal::Lit(u64::from(a > b)))
        }
        Prim::Le(_) => {
            let (a, b) = eval_bin_args(eval_arena, globals, env, args)?;
            Ok(MetaVal::Lit(u64::from(a <= b)))
        }
        Prim::Ge(_) => {
            let (a, b) = eval_bin_args(eval_arena, globals, env, args)?;
            Ok(MetaVal::Lit(u64::from(a >= b)))
        }

        // ── Embed: meta integer → object code ─────────────────────────────────
        Prim::Embed(width) => {
            let [arg] = args else {
                panic!("unary primitive requires exactly 1 argument (typechecker invariant)")
            };
            let n = eval_lit(eval_arena, globals, env, arg)?;
            let phase = Phase::Object;
            let lit_val = eval_arena.alloc(ObjVal::Lit(n, IntType { width, phase }));
            Ok(MetaVal::Quote(lit_val))
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
fn eval_meta_match<'names, 'eval>(
    eval_arena: &'eval Bump,
    globals: &Globals<'names, 'eval>,
    env: &mut Env<'names, 'eval>,
    n: u64,
    arms: &'eval [Arm<'names, 'eval>],
) -> Result<MetaVal<'names, 'eval>> {
    for arm in arms {
        match &arm.pat {
            Pat::Lit(m) => {
                if n == *m {
                    return eval_meta(eval_arena, globals, env, arm.body);
                }
            }
            Pat::Bind(_) | Pat::Wildcard => {
                env.push_meta(MetaVal::Lit(n));
                let result = eval_meta(eval_arena, globals, env, arm.body);
                env.pop();
                return result;
            }
        }
    }
    Err(anyhow!(
        "non-exhaustive match during staging (scrutinee = {n})"
    ))
}

// ── Object-level evaluator ────────────────────────────────────────────────────

/// Evaluate an object-level `term` to an `ObjVal`, eliminating all `Splice` nodes.
///
/// Variables are stored as De Bruijn levels so the result can be spliced into any
/// depth without index shifting.  Call `quote_obj` to convert back to a `Term`.
///
/// All `ObjVal` nodes are allocated in `eval_arena` (a temporary arena); they are
/// discarded once `quote_obj` has converted them to output `Term` nodes.
fn eval_obj<'names, 'eval>(
    eval_arena: &'eval Bump,
    globals: &Globals<'names, 'eval>,
    env: &mut Env<'names, 'eval>,
    term: &'eval Term<'names, 'eval>,
) -> Result<&'eval ObjVal<'names, 'eval>> {
    match term {
        // ── Variable ─────────────────────────────────────────────────────────
        Term::Var(ix) => match env.get_ix(*ix) {
            Binding::Obj(lvl) => Ok(eval_arena.alloc(ObjVal::Var(*lvl))),
            // A spliced code value stored as a Quote: return the ObjVal directly.
            // No index shifting needed — the value uses absolute levels.
            Binding::Meta(MetaVal::Quote(v)) => Ok(v),
            Binding::Meta(MetaVal::Lit(_)) => unreachable!(
                "integer meta variable at index {} referenced in object context \
                 (typechecker invariant)",
                ix.as_usize()
            ),
            Binding::Meta(MetaVal::Closure { .. }) => unreachable!(
                "closure meta variable at index {:?} referenced in object context \
                 (typechecker invariant)",
                ix
            ),
            Binding::Meta(MetaVal::Ty) => unreachable!(
                "type meta variable at index {:?} referenced in object context \
                 (typechecker invariant)",
                ix
            ),
        },

        // ── Literal ──────────────────────────────────────────────────────────
        Term::Lit(n, it) => Ok(eval_arena.alloc(ObjVal::Lit(*n, *it))),

        // ── Primitive ────────────────────────────────────────────────────────
        Term::Prim(p) => Ok(eval_arena.alloc(ObjVal::Prim(*p))),

        // ── Global reference ─────────────────────────────────────────────────
        Term::Global(name) => Ok(eval_arena.alloc(ObjVal::Global(name))),

        // ── App ───────────────────────────────────────────────────────────────
        Term::App(app) => {
            let func_val = eval_obj(eval_arena, globals, env, app.func)?;
            let arg_vals: &'eval [ObjVal<'names, 'eval>] = eval_arena.alloc_slice_try_fill_iter(
                app.args
                    .iter()
                    .map(|arg| eval_obj(eval_arena, globals, env, arg).cloned()),
            )?;
            Ok(eval_arena.alloc(ObjVal::App(func_val, arg_vals)))
        }

        // ── Splice: $(t) — the key staging step ──────────────────────────────
        Term::Splice(inner) => {
            let meta_val = eval_meta(eval_arena, globals, env, inner)?;
            match meta_val {
                // Return the ObjVal directly — no index shifting, levels are absolute.
                MetaVal::Quote(v) => Ok(v),
                MetaVal::Lit(_) | MetaVal::Ty | MetaVal::Closure { .. } => {
                    unreachable!("splice evaluated to non-code value (typechecker invariant)")
                }
            }
        }

        // ── Let binding ──────────────────────────────────────────────────────
        Term::Let(let_) => {
            let ty_val = eval_obj(eval_arena, globals, env, let_.ty)?;
            let expr_val = eval_obj(eval_arena, globals, env, let_.expr)?;
            env.push_obj();
            let body_val = eval_obj(eval_arena, globals, env, let_.body);
            env.pop();
            Ok(eval_arena.alloc(ObjVal::Let {
                name: let_.name,
                ty: ty_val,
                expr: expr_val,
                body: body_val?,
            }))
        }

        // ── Match ────────────────────────────────────────────────────────────
        Term::Match(match_) => {
            let scrutinee_val = eval_obj(eval_arena, globals, env, match_.scrutinee)?;
            let arm_vals: &'eval [ObjArm<'_, 'eval>] = eval_arena.alloc_slice_try_fill_iter(
                match_.arms.iter().map(|arm| -> Result<_> {
                    let has_binding = arm.pat.bound_name().is_some();
                    if has_binding {
                        env.push_obj();
                    }
                    let body_val = eval_obj(eval_arena, globals, env, arm.body);
                    if has_binding {
                        env.pop();
                    }
                    Ok(ObjArm {
                        pat: arm.pat.clone(),
                        body: body_val?,
                    })
                }),
            )?;
            Ok(eval_arena.alloc(ObjVal::Match {
                scrutinee: scrutinee_val,
                arms: arm_vals,
            }))
        }

        // ── Unreachable in well-typed object terms ───────────────────────────
        Term::Quote(_) => unreachable!("Quote in object context (typechecker invariant)"),
        Term::Lift(_) | Term::Pi(_) | Term::Lam(_) => {
            unreachable!("meta-only term in object context (typechecker invariant)")
        }
    }
}

// ── Object-level readback ─────────────────────────────────────────────────────

/// Convert an `ObjVal` back to a `Term` by translating De Bruijn levels to indices.
///
/// `depth` is the number of object-level variables currently in scope.
fn quote_obj<'names, 'out>(
    out_arena: &'out Bump,
    depth: de_bruijn::Depth,
    val: &ObjVal<'names, '_>,
) -> &'out Term<'names, 'out> {
    match val {
        ObjVal::Var(lvl) => out_arena.alloc(Term::Var(lvl.ix_at(depth))),
        ObjVal::Lit(n, it) => out_arena.alloc(Term::Lit(*n, *it)),
        ObjVal::Prim(p) => out_arena.alloc(Term::Prim(*p)),
        ObjVal::Global(name) => out_arena.alloc(Term::Global(name)),
        ObjVal::App(func, args) => {
            let qfunc = quote_obj(out_arena, depth, func);
            let qargs = out_arena
                .alloc_slice_fill_iter(args.iter().map(|a| quote_obj(out_arena, depth, a)));
            out_arena.alloc(Term::new_app(qfunc, qargs))
        }
        ObjVal::Let {
            name,
            ty,
            expr,
            body,
        } => {
            let qty = quote_obj(out_arena, depth, ty);
            let qexpr = quote_obj(out_arena, depth, expr);
            let qbody = quote_obj(out_arena, depth.succ(), body);
            out_arena.alloc(Term::new_let(name, qty, qexpr, qbody))
        }
        ObjVal::Match { scrutinee, arms } => {
            let qscrutinee = quote_obj(out_arena, depth, scrutinee);
            let qarms = out_arena.alloc_slice_fill_iter(arms.iter().map(|arm| {
                let arm_depth = if arm.pat.bound_name().is_some() {
                    depth.succ()
                } else {
                    depth
                };
                let out_pat = match &arm.pat {
                    Pat::Lit(n) => Pat::Lit(*n),
                    Pat::Bind(name) => Pat::Bind(name),
                    Pat::Wildcard => Pat::Wildcard,
                };
                Arm {
                    pat: out_pat,
                    body: quote_obj(out_arena, arm_depth, arm.body),
                }
            }));
            out_arena.alloc(Term::new_match(qscrutinee, qarms))
        }
    }
}

// ── Public entry point ────────────────────────────────────────────────────────

/// Unstage an elaborated program.
///
/// - `out_arena`: output arena; the returned `Program` is allocated here.
/// - `program`: input core program; may be dropped once this function returns.
pub fn unstage_program<'names, 'out, 'core>(
    out_arena: &'out Bump,
    program: &'core Program<'names, 'core>,
) -> Result<Program<'names, 'out>> {
    // Build meta globals table: only meta-level definitions are unfolded during staging.
    let globals: Globals<'names, '_> = program
        .defs
        .iter()
        .filter_map(|f| match &f.global {
            core::Global::Meta(meta) => Some((f.name, meta.body)),
            core::Global::CodeFn(_) => None,
        })
        .collect();

    let staged_defs: Vec<core::GlobalDef<'names, 'out>> = program
        .defs
        .iter()
        .filter_map(|f| match &f.global {
            core::Global::CodeFn(codefn) => Some((f.name, codefn)),
            core::Global::Meta(_) => None,
        })
        .map(|(name, codefn)| -> Result<_> {
            // Per-definition eval arena: all intermediate `ObjVal` nodes are
            // allocated here and freed automatically when this closure returns.
            let eval_arena = Bump::new();
            let mut env = Env::new(de_bruijn::Depth::ZERO);

            let staged_params =
                out_arena.alloc_slice_try_fill_iter(codefn.params.iter().map(
                    |(n, ty)| -> Result<(&'names Name, &'out Term<'names, 'out>)> {
                        let ty_val = eval_obj(&eval_arena, &globals, &mut env, ty)?;
                        let staged_ty = quote_obj(out_arena, env.obj_depth, ty_val);
                        env.push_obj();
                        Ok((n, staged_ty))
                    },
                ))?;

            let ret_ty_val = eval_obj(&eval_arena, &globals, &mut env, codefn.ret_ty)?;
            let staged_ret_ty = quote_obj(out_arena, env.obj_depth, ret_ty_val);

            let body_val = eval_obj(&eval_arena, &globals, &mut env, codefn.body)?;
            let staged_body = quote_obj(out_arena, env.obj_depth, body_val);

            Ok(core::GlobalDef {
                name,
                global: core::Global::CodeFn(core::CodeFn {
                    params: staged_params,
                    ret_ty: staged_ret_ty,
                    body: staged_body,
                }),
            })
        })
        .collect::<Result<Vec<_>>>()?;

    let defs = out_arena.alloc_slice_fill_iter(staged_defs);
    Ok(Program { defs })
}
