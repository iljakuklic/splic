use std::collections::HashMap;

use anyhow::{Result, anyhow, ensure};
use bumpalo::Bump;

use crate::common::de_bruijn;
use crate::core::{Arm, Function, IntType, IntWidth, Name, Pat, Pi, Prim, Program, Term};
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
enum ObjVal<'a> {
    /// Local variable identified by De Bruijn level (absolute, context-independent).
    Var(de_bruijn::Lvl),
    /// Integer literal.
    Lit(u64, IntType),
    /// Unapplied primitive.
    Prim(Prim),
    /// Global function reference.
    Global(&'a Name),
    /// Application.
    App(&'a Self, &'a [Self]),
    /// Let binding.
    Let {
        name: &'a Name,
        ty: &'a Self,
        expr: &'a Self,
        body: &'a Self,
    },
    /// Pattern match.
    Match {
        scrutinee: &'a Self,
        arms: &'a [ObjArm<'a>],
    },
}

#[derive(Clone, Debug)]
struct ObjArm<'a> {
    pat: Pat<'a>,
    body: &'a ObjVal<'a>,
}

// ── Meta-level values ─────────────────────────────────────────────────────────

/// A value produced by meta-level evaluation.
///
/// Two lifetime parameters:
/// - `'out`: lifetime of the output arena (for `Quote` values that appear in the result).
/// - `'eval`: lifetime of the evaluation phase — covers both the input program data (`'core`)
///   and any temporary terms allocated in the local eval arena.  Since `Term` is covariant
///   in its lifetime, `'core` data can be coercible to `'eval` at call sites.
#[derive(Clone, Debug)]
enum MetaVal<'out, 'eval> {
    /// A concrete integer value computed at meta (compile) time.
    Lit(u64),
    /// Quoted object-level code as a semantic value.
    ///
    /// Uses De Bruijn levels internally, so it can be spliced into any depth without adjustment.
    Quote(&'out ObjVal<'out>),
    /// A type term passed as a type argument (dependent types: types are values).
    /// The type term itself is not inspected during evaluation.
    Ty,
    /// A closure: a lambda body captured with its environment.
    Closure {
        body: &'eval Term<'eval>,
        arity: usize,
        env: Vec<Binding<'out, 'eval>>,
        obj_depth: de_bruijn::Depth,
    },
}

// ── Environment ───────────────────────────────────────────────────────────────

/// A binding stored in the evaluation environment, indexed by De Bruijn level.
#[derive(Clone, Debug)]
enum Binding<'out, 'eval> {
    /// A meta-level variable bound to a concrete `MetaVal`.
    Meta(MetaVal<'out, 'eval>),
    /// An object-level variable.
    Obj(de_bruijn::Lvl),
}

/// Evaluation environment: a stack of bindings indexed by De Bruijn index.
///
/// Bindings are stored oldest-first. `Var(Ix(i))` refers to
/// `bindings[bindings.len() - 1 - i]` — the `i`-th binding from the end.
#[derive(Debug)]
struct Env<'out, 'eval> {
    bindings: Vec<Binding<'out, 'eval>>,
    obj_depth: de_bruijn::Depth,
}

impl<'out, 'eval> Env<'out, 'eval> {
    const fn new(obj_depth: de_bruijn::Depth) -> Self {
        Env {
            bindings: Vec::new(),
            obj_depth,
        }
    }

    /// Look up the binding for `Var(Ix(ix))`.
    fn get_ix(&self, ix: de_bruijn::Ix) -> &Binding<'out, 'eval> {
        let depth = de_bruijn::Depth::new(self.bindings.len());
        let lvl = ix.lvl_at(depth);
        let i = lvl.as_usize();
        self.bindings
            .get(i)
            .expect("De Bruijn index out of environment bounds")
    }

    /// Push an object-level binding.
    fn push_obj(&mut self) {
        let lvl = self.obj_depth.as_lvl();
        self.obj_depth = self.obj_depth.succ();
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
                self.obj_depth = de_bruijn::Depth::new(
                    self.obj_depth
                        .as_usize()
                        .checked_sub(1)
                        .expect("obj_depth underflow on pop"),
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
                "object variable at index {:?} referenced in meta context (typechecker invariant)",
                ix
            ),
        },

        // ── Literal ──────────────────────────────────────────────────────────
        Term::Lit(n, _) => Ok(MetaVal::Lit(*n)),

        // ── Global reference ─────────────────────────────────────────────────
        Term::Global(name) => {
            let def = globals
                .get(name)
                .unwrap_or_else(|| panic!("unknown global `{name}` during staging"));
            Ok(global_to_closure(def, env.obj_depth))
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
            Term::Prim(prim) => eval_meta_prim(arena, eval_arena, globals, env, *prim, app.args),
            _ => {
                let func_val = eval_meta(arena, eval_arena, globals, env, app.func)?;
                let arg_vals: Vec<MetaVal<'out, 'eval>> = app
                    .args
                    .iter()
                    .map(|a| eval_meta(arena, eval_arena, globals, env, a))
                    .collect::<Result<_>>()?;
                apply_closure_n(arena, eval_arena, globals, func_val, &arg_vals)
            }
        },

        // ── Quote: #(t) ──────────────────────────────────────────────────────
        Term::Quote(inner) => {
            let obj_val = eval_obj(arena, eval_arena, globals, env, inner)?;
            Ok(MetaVal::Quote(obj_val))
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
                MetaVal::Quote(_) | MetaVal::Ty | MetaVal::Closure { .. } => unreachable!(
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
const fn global_to_closure<'out, 'eval>(
    def: &GlobalDef<'eval>,
    obj_depth: de_bruijn::Depth,
) -> MetaVal<'out, 'eval> {
    MetaVal::Closure {
        body: def.body,
        arity: def.ty.params.len(),
        env: Vec::new(),
        obj_depth,
    }
}

/// Apply a closure value to N arguments simultaneously.
fn apply_closure_n<'out, 'eval>(
    arena: &'out Bump,
    eval_arena: &'eval Bump,
    globals: &Globals<'eval>,
    func_val: MetaVal<'out, 'eval>,
    args: &[MetaVal<'out, 'eval>],
) -> Result<MetaVal<'out, 'eval>> {
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
            eval_meta(arena, eval_arena, globals, &mut callee_env, body)
        }
        MetaVal::Lit(_) | MetaVal::Quote(_) | MetaVal::Ty => {
            unreachable!("applying a non-function value (typechecker invariant)")
        }
    }
}

fn eval_lit<'out, 'eval>(
    arena: &'out Bump,
    eval_arena: &'eval Bump,
    globals: &Globals<'eval>,
    env: &mut Env<'out, 'eval>,
    arg: &'eval Term<'eval>,
) -> Result<u64> {
    eval_meta(arena, eval_arena, globals, env, arg).map(|v| match v {
        MetaVal::Lit(n) => n,
        MetaVal::Quote(_) | MetaVal::Ty | MetaVal::Closure { .. } => unreachable!(
            "expected integer meta value for primitive operand (typechecker invariant)"
        ),
    })
}

fn eval_bin_args<'out, 'eval>(
    arena: &'out Bump,
    eval_arena: &'eval Bump,
    globals: &Globals<'eval>,
    env: &mut Env<'out, 'eval>,
    args: &'eval [&'eval Term<'eval>],
) -> Result<(u64, u64)> {
    let [lhs, rhs] = args else {
        panic!("binary primitive requires exactly 2 arguments (typechecker invariant)")
    };
    let a = eval_lit(arena, eval_arena, globals, env, lhs)?;
    let b = eval_lit(arena, eval_arena, globals, env, rhs)?;
    Ok((a, b))
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
    match prim {
        // ── Arithmetic ────────────────────────────────────────────────────────
        Prim::Add(IntType { width, .. }) => {
            let (a, b) = eval_bin_args(arena, eval_arena, globals, env, args)?;
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
            let (a, b) = eval_bin_args(arena, eval_arena, globals, env, args)?;
            let result = a.checked_sub(b).ok_or_else(|| {
                anyhow!(
                    "arithmetic overflow during staging: \
                     {a} - {b} underflows {width}"
                )
            })?;
            Ok(MetaVal::Lit(result))
        }
        Prim::Mul(IntType { width, .. }) => {
            let (a, b) = eval_bin_args(arena, eval_arena, globals, env, args)?;
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
            let (a, b) = eval_bin_args(arena, eval_arena, globals, env, args)?;
            ensure!(b != 0, "division by zero during staging");
            Ok(MetaVal::Lit(a / b))
        }

        // ── Bitwise ───────────────────────────────────────────────────────────
        Prim::BitAnd(_) => {
            let (a, b) = eval_bin_args(arena, eval_arena, globals, env, args)?;
            Ok(MetaVal::Lit(a & b))
        }
        Prim::BitOr(_) => {
            let (a, b) = eval_bin_args(arena, eval_arena, globals, env, args)?;
            Ok(MetaVal::Lit(a | b))
        }
        Prim::BitNot(IntType { width, .. }) => {
            let [arg] = args else {
                panic!("unary primitive requires exactly 1 argument (typechecker invariant)")
            };
            let a = eval_lit(arena, eval_arena, globals, env, arg)?;
            Ok(MetaVal::Lit(mask_to_width(width, !a)))
        }

        // ── Comparison ────────────────────────────────────────────────────────
        Prim::Eq(_) => {
            let (a, b) = eval_bin_args(arena, eval_arena, globals, env, args)?;
            Ok(MetaVal::Lit(u64::from(a == b)))
        }
        Prim::Ne(_) => {
            let (a, b) = eval_bin_args(arena, eval_arena, globals, env, args)?;
            Ok(MetaVal::Lit(u64::from(a != b)))
        }
        Prim::Lt(_) => {
            let (a, b) = eval_bin_args(arena, eval_arena, globals, env, args)?;
            Ok(MetaVal::Lit(u64::from(a < b)))
        }
        Prim::Gt(_) => {
            let (a, b) = eval_bin_args(arena, eval_arena, globals, env, args)?;
            Ok(MetaVal::Lit(u64::from(a > b)))
        }
        Prim::Le(_) => {
            let (a, b) = eval_bin_args(arena, eval_arena, globals, env, args)?;
            Ok(MetaVal::Lit(u64::from(a <= b)))
        }
        Prim::Ge(_) => {
            let (a, b) = eval_bin_args(arena, eval_arena, globals, env, args)?;
            Ok(MetaVal::Lit(u64::from(a >= b)))
        }

        // ── Embed: meta integer → object code ─────────────────────────────────
        Prim::Embed(width) => {
            let [arg] = args else {
                panic!("unary primitive requires exactly 1 argument (typechecker invariant)")
            };
            let n = eval_lit(arena, eval_arena, globals, env, arg)?;
            let phase = Phase::Object;
            let lit_val = arena.alloc(ObjVal::Lit(n, IntType { width, phase }));
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

// ── Object-level evaluator ────────────────────────────────────────────────────

/// Evaluate an object-level `term` to an `ObjVal`, eliminating all `Splice` nodes.
///
/// Variables are stored as De Bruijn levels so the result can be spliced into any
/// depth without index shifting.  Call `quote_obj` to convert back to a `Term`.
fn eval_obj<'out, 'eval>(
    arena: &'out Bump,
    eval_arena: &'eval Bump,
    globals: &Globals<'eval>,
    env: &mut Env<'out, 'eval>,
    term: &'eval Term<'eval>,
) -> Result<&'out ObjVal<'out>> {
    match term {
        // ── Variable ─────────────────────────────────────────────────────────
        Term::Var(ix) => match env.get_ix(*ix) {
            Binding::Obj(lvl) => Ok(arena.alloc(ObjVal::Var(*lvl))),
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
        Term::Lit(n, it) => Ok(arena.alloc(ObjVal::Lit(*n, *it))),

        // ── Primitive ────────────────────────────────────────────────────────
        Term::Prim(p) => Ok(arena.alloc(ObjVal::Prim(*p))),

        // ── Global reference ─────────────────────────────────────────────────
        Term::Global(name) => {
            Ok(arena.alloc(ObjVal::Global(Name::new(arena.alloc_str(name.as_str())))))
        }

        // ── App ───────────────────────────────────────────────────────────────
        Term::App(app) => {
            let func_val = eval_obj(arena, eval_arena, globals, env, app.func)?;
            let arg_vals: &'out [ObjVal<'out>] = arena.alloc_slice_try_fill_iter(
                app.args
                    .iter()
                    .map(|arg| eval_obj(arena, eval_arena, globals, env, arg).cloned()),
            )?;
            Ok(arena.alloc(ObjVal::App(func_val, arg_vals)))
        }

        // ── Splice: $(t) — the key staging step ──────────────────────────────
        Term::Splice(inner) => {
            let meta_val = eval_meta(arena, eval_arena, globals, env, inner)?;
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
            let ty_val = eval_obj(arena, eval_arena, globals, env, let_.ty)?;
            let expr_val = eval_obj(arena, eval_arena, globals, env, let_.expr)?;
            env.push_obj();
            let body_val = eval_obj(arena, eval_arena, globals, env, let_.body);
            env.pop();
            Ok(arena.alloc(ObjVal::Let {
                name: Name::new(arena.alloc_str(let_.name.as_str())),
                ty: ty_val,
                expr: expr_val,
                body: body_val?,
            }))
        }

        // ── Match ────────────────────────────────────────────────────────────
        Term::Match(match_) => {
            let scrutinee_val = eval_obj(arena, eval_arena, globals, env, match_.scrutinee)?;
            let arm_vals: &'out [ObjArm<'out>] =
                arena.alloc_slice_try_fill_iter(match_.arms.iter().map(|arm| -> Result<_> {
                    let pat = match &arm.pat {
                        Pat::Lit(n) => Pat::Lit(*n),
                        Pat::Bind(name) => Pat::Bind(Name::new(arena.alloc_str(name.as_str()))),
                        Pat::Wildcard => Pat::Wildcard,
                    };
                    let has_binding = arm.pat.bound_name().is_some();
                    if has_binding {
                        env.push_obj();
                    }
                    let body_val = eval_obj(arena, eval_arena, globals, env, arm.body);
                    if has_binding {
                        env.pop();
                    }
                    Ok(ObjArm {
                        pat,
                        body: body_val?,
                    })
                }))?;
            Ok(arena.alloc(ObjVal::Match {
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
fn quote_obj<'out>(
    arena: &'out Bump,
    depth: de_bruijn::Depth,
    val: &ObjVal<'out>,
) -> &'out Term<'out> {
    match val {
        ObjVal::Var(lvl) => arena.alloc(Term::Var(lvl.ix_at(depth))),
        ObjVal::Lit(n, it) => arena.alloc(Term::Lit(*n, *it)),
        ObjVal::Prim(p) => arena.alloc(Term::Prim(*p)),
        ObjVal::Global(name) => arena.alloc(Term::Global(name)),
        ObjVal::App(func, args) => {
            let qfunc = quote_obj(arena, depth, func);
            let qargs =
                arena.alloc_slice_fill_iter(args.iter().map(|a| quote_obj(arena, depth, a)));
            arena.alloc(Term::new_app(qfunc, qargs))
        }
        ObjVal::Let {
            name,
            ty,
            expr,
            body,
        } => {
            let qty = quote_obj(arena, depth, ty);
            let qexpr = quote_obj(arena, depth, expr);
            let qbody = quote_obj(arena, depth.succ(), body);
            arena.alloc(Term::new_let(name, qty, qexpr, qbody))
        }
        ObjVal::Match { scrutinee, arms } => {
            let qscrutinee = quote_obj(arena, depth, scrutinee);
            let qarms = arena.alloc_slice_fill_iter(arms.iter().map(|arm| {
                let arm_depth = if arm.pat.bound_name().is_some() {
                    depth.succ()
                } else {
                    depth
                };
                Arm {
                    pat: arm.pat.clone(),
                    body: quote_obj(arena, arm_depth, arm.body),
                }
            }));
            arena.alloc(Term::new_match(qscrutinee, qarms))
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
    let eval_bump = Bump::new();

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
            let mut env = Env::new(de_bruijn::Depth::ZERO);

            let staged_params = arena.alloc_slice_try_fill_iter(pi.params.iter().map(
                |(n, ty)| -> Result<(&'out Name, &'out Term<'out>)> {
                    let ty_val = eval_obj(arena, &eval_bump, &globals, &mut env, ty)?;
                    let staged_ty = quote_obj(arena, env.obj_depth, ty_val);
                    env.push_obj();
                    Ok((Name::new(arena.alloc_str(n.as_str())), staged_ty))
                },
            ))?;

            let ret_ty_val = eval_obj(arena, &eval_bump, &globals, &mut env, pi.body_ty)?;
            let staged_ret_ty = quote_obj(arena, env.obj_depth, ret_ty_val);

            let body_val = eval_obj(arena, &eval_bump, &globals, &mut env, f.body)?;
            let staged_body = quote_obj(arena, env.obj_depth, body_val);

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
