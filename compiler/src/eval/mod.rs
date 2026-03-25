use std::collections::HashMap;

use anyhow::{Result, anyhow, ensure};
use bumpalo::Bump;

use crate::core::{
    Arm, FunApp, FunSig, Function, IntType, IntWidth, Lam, Lvl, Name, Pat, Prim, Program, Term,
};
use crate::parser::ast::Phase;

// ── Value types ───────────────────────────────────────────────────────────────

/// A value produced by meta-level evaluation.
#[derive(Clone, Debug)]
enum MetaVal<'out> {
    /// A concrete integer value computed at meta (compile) time.
    Lit(u64),
    /// Quoted object-level code.
    Code(&'out Term<'out>),
    /// A type term passed as a type argument (dependent types: types are values).
    /// The type term itself is not inspected during evaluation.
    Ty,
    /// A closure: a lambda body captured with its environment.
    Closure {
        body: &'out Term<'out>,
        env: Vec<Binding<'out>>,
        obj_next: Lvl,
    },
}

// ── Environment ───────────────────────────────────────────────────────────────

/// A binding stored in the evaluation environment, indexed by De Bruijn level.
#[derive(Clone, Debug)]
enum Binding<'out> {
    /// A meta-level variable bound to a concrete `MetaVal`.
    Meta(MetaVal<'out>),
    /// An object-level variable.
    Obj(Lvl),
}

/// Evaluation environment: a stack of bindings indexed by De Bruijn level.
#[derive(Debug)]
struct Env<'out> {
    bindings: Vec<Binding<'out>>,
    obj_next: Lvl,
}

impl<'out> Env<'out> {
    const fn new(obj_next: Lvl) -> Self {
        Env {
            bindings: Vec::new(),
            obj_next,
        }
    }

    /// Look up the binding at level `lvl`.
    fn get(&self, lvl: Lvl) -> &Binding<'out> {
        self.bindings
            .get(lvl.0)
            .expect("De Bruijn level in env bounds")
    }

    /// Push an object-level binding.
    fn push_obj(&mut self) {
        let lvl = self.obj_next;
        self.obj_next = lvl.succ();
        self.bindings.push(Binding::Obj(lvl));
    }

    /// Push a meta-level binding bound to the given value.
    fn push_meta(&mut self, val: MetaVal<'out>) {
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
    sig: &'a FunSig<'a>,
    body: &'a Term<'a>,
}

type Globals<'a> = HashMap<Name<'a>, GlobalDef<'a>>;

// ── Meta-level evaluator ──────────────────────────────────────────────────────

/// Evaluate a meta-level `term` to a `MetaVal`.
fn eval_meta<'out>(
    arena: &'out Bump,
    globals: &Globals<'out>,
    env: &mut Env<'out>,
    term: &'out Term<'out>,
) -> Result<MetaVal<'out>> {
    match term {
        // ── Variable ─────────────────────────────────────────────────────────
        Term::Var(lvl) => match env.get(*lvl) {
            Binding::Meta(v) => Ok(v.clone()),
            Binding::Obj(_) => unreachable!(
                "object variable at level {} referenced in meta context (typechecker invariant)",
                lvl.0
            ),
        },

        // ── Literal ──────────────────────────────────────────────────────────
        Term::Lit(n, _) => Ok(MetaVal::Lit(*n)),

        // ── Global reference ─────────────────────────────────────────────────
        Term::Global(name) => {
            let def = globals
                .get(name)
                .unwrap_or_else(|| panic!("unknown global `{name}` during staging"));
            if def.sig.params.is_empty() {
                // Zero-param global: evaluate the body immediately in a fresh env.
                // (Zero-param Pi types don't exist, so zero-param globals are always
                // called, never passed as values.)
                let mut callee_env = Env::new(env.obj_next);
                eval_meta(arena, globals, &mut callee_env, def.body)
            } else {
                // Multi-param global: produce a closure, capturing the caller's
                // obj_next so object-level let bindings inside quotes don't clash
                // with output levels already in use at the call site.
                Ok(global_to_closure(arena, def, env.obj_next))
            }
        }

        // ── Lambda ───────────────────────────────────────────────────────────
        Term::Lam(lam) => Ok(MetaVal::Closure {
            body: lam.body,
            env: env.bindings.clone(),
            obj_next: env.obj_next,
        }),

        // ── Function application ─────────────────────────────────────────────
        Term::FunApp(app) => {
            let func_val = eval_meta(arena, globals, env, app.func)?;
            let arg_val = eval_meta(arena, globals, env, app.arg)?;
            apply_closure(arena, globals, func_val, arg_val)
        }

        // ── PrimApp ──────────────────────────────────────────────────────────
        Term::PrimApp(app) => eval_meta_prim(arena, globals, env, app.prim, app.args),

        // ── Quote: #(t) ──────────────────────────────────────────────────────
        Term::Quote(inner) => {
            let obj_term = unstage_obj(arena, globals, env, inner)?;
            Ok(MetaVal::Code(obj_term))
        }

        // ── Let binding ──────────────────────────────────────────────────────
        Term::Let(let_) => {
            let val = eval_meta(arena, globals, env, let_.expr)?;
            env.push_meta(val);
            let result = eval_meta(arena, globals, env, let_.body);
            env.pop();
            result
        }

        // ── Match ────────────────────────────────────────────────────────────
        Term::Match(match_) => {
            let scrut_val = eval_meta(arena, globals, env, match_.scrutinee)?;
            let n = match scrut_val {
                MetaVal::Lit(n) => n,
                MetaVal::Code(_) | MetaVal::Ty | MetaVal::Closure { .. } => unreachable!(
                    "cannot match on non-integer at meta level (typechecker invariant)"
                ),
            };
            eval_meta_match(arena, globals, env, n, match_.arms)
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
/// For a multi-parameter function, we build nested closures. E.g., `fn f(x, y) = body`
/// becomes a closure whose body is a lambda `|y| body`.
fn global_to_closure<'out>(
    arena: &'out Bump,
    def: &GlobalDef<'out>,
    obj_next: Lvl,
) -> MetaVal<'out> {
    let params = def.sig.params;
    if params.is_empty() {
        MetaVal::Closure {
            body: def.body,
            env: Vec::new(),
            obj_next,
        }
    } else {
        // Build nested lambdas for params[1..], then wrap in a closure for params[0].
        let mut body: &Term = def.body;
        for &(name, ty) in params.iter().rev().skip(1) {
            body = arena.alloc(Term::Lam(Lam {
                param_name: name,
                param_ty: ty,
                body,
            }));
        }
        MetaVal::Closure {
            body,
            env: Vec::new(),
            obj_next,
        }
    }
}

/// Apply a closure value to an argument value.
fn apply_closure<'out>(
    arena: &'out Bump,
    globals: &Globals<'out>,
    func_val: MetaVal<'out>,
    arg_val: MetaVal<'out>,
) -> Result<MetaVal<'out>> {
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
            
            // Restore env for the caller (bindings are consumed by the callee).
            eval_meta(arena, globals, &mut callee_env, body)
        }
        MetaVal::Lit(_) | MetaVal::Code(_) | MetaVal::Ty => {
            unreachable!("applying a non-function value (typechecker invariant)")
        }
    }
}

/// Evaluate a primitive operation at meta level.
fn eval_meta_prim<'out>(
    arena: &'out Bump,
    globals: &Globals<'out>,
    env: &mut Env<'out>,
    prim: Prim,
    args: &'out [&'out Term<'out>],
) -> Result<MetaVal<'out>> {
    let eval_lit =
        |arena: &'out Bump, globals: &Globals<'out>, env: &mut Env<'out>, arg: &'out Term<'out>| {
            eval_meta(arena, globals, env, arg).map(|v| match v {
                MetaVal::Lit(n) => n,
                MetaVal::Code(_) | MetaVal::Ty | MetaVal::Closure { .. } => unreachable!(
                    "expected integer meta value for primitive operand (typechecker invariant)"
                ),
            })
        };

    #[expect(clippy::indexing_slicing)]
    match prim {
        // ── Arithmetic ────────────────────────────────────────────────────────
        Prim::Add(IntType { width, .. }) => {
            let a = eval_lit(arena, globals, env, args[0])?;
            let b = eval_lit(arena, globals, env, args[1])?;
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
            let a = eval_lit(arena, globals, env, args[0])?;
            let b = eval_lit(arena, globals, env, args[1])?;
            let result = a.checked_sub(b).ok_or_else(|| {
                anyhow!(
                    "arithmetic overflow during staging: \
                     {a} - {b} underflows {width}"
                )
            })?;
            Ok(MetaVal::Lit(result))
        }
        Prim::Mul(IntType { width, .. }) => {
            let a = eval_lit(arena, globals, env, args[0])?;
            let b = eval_lit(arena, globals, env, args[1])?;
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
            let a = eval_lit(arena, globals, env, args[0])?;
            let b = eval_lit(arena, globals, env, args[1])?;
            ensure!(b != 0, "division by zero during staging");
            Ok(MetaVal::Lit(a / b))
        }

        // ── Bitwise ───────────────────────────────────────────────────────────
        Prim::BitAnd(_) => {
            let a = eval_lit(arena, globals, env, args[0])?;
            let b = eval_lit(arena, globals, env, args[1])?;
            Ok(MetaVal::Lit(a & b))
        }
        Prim::BitOr(_) => {
            let a = eval_lit(arena, globals, env, args[0])?;
            let b = eval_lit(arena, globals, env, args[1])?;
            Ok(MetaVal::Lit(a | b))
        }
        Prim::BitNot(IntType { width, .. }) => {
            let a = eval_lit(arena, globals, env, args[0])?;
            Ok(MetaVal::Lit(mask_to_width(width, !a)))
        }

        // ── Comparison ────────────────────────────────────────────────────────
        Prim::Eq(_) => {
            let a = eval_lit(arena, globals, env, args[0])?;
            let b = eval_lit(arena, globals, env, args[1])?;
            Ok(MetaVal::Lit(u64::from(a == b)))
        }
        Prim::Ne(_) => {
            let a = eval_lit(arena, globals, env, args[0])?;
            let b = eval_lit(arena, globals, env, args[1])?;
            Ok(MetaVal::Lit(u64::from(a != b)))
        }
        Prim::Lt(_) => {
            let a = eval_lit(arena, globals, env, args[0])?;
            let b = eval_lit(arena, globals, env, args[1])?;
            Ok(MetaVal::Lit(u64::from(a < b)))
        }
        Prim::Gt(_) => {
            let a = eval_lit(arena, globals, env, args[0])?;
            let b = eval_lit(arena, globals, env, args[1])?;
            Ok(MetaVal::Lit(u64::from(a > b)))
        }
        Prim::Le(_) => {
            let a = eval_lit(arena, globals, env, args[0])?;
            let b = eval_lit(arena, globals, env, args[1])?;
            Ok(MetaVal::Lit(u64::from(a <= b)))
        }
        Prim::Ge(_) => {
            let a = eval_lit(arena, globals, env, args[0])?;
            let b = eval_lit(arena, globals, env, args[1])?;
            Ok(MetaVal::Lit(u64::from(a >= b)))
        }

        // ── Embed: meta integer → object code ─────────────────────────────────
        Prim::Embed(width) => {
            let n = eval_lit(arena, globals, env, args[0])?;
            let phase = Phase::Object;
            let lit_term = arena.alloc(Term::Lit(n, IntType { width, phase }));
            Ok(MetaVal::Code(lit_term))
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
fn eval_meta_match<'out>(
    arena: &'out Bump,
    globals: &Globals<'out>,
    env: &mut Env<'out>,
    n: u64,
    arms: &'out [Arm<'out>],
) -> Result<MetaVal<'out>> {
    for arm in arms {
        match &arm.pat {
            Pat::Lit(m) => {
                if n == *m {
                    return eval_meta(arena, globals, env, arm.body);
                }
            }
            Pat::Bind(_) | Pat::Wildcard => {
                env.push_meta(MetaVal::Lit(n));
                let result = eval_meta(arena, globals, env, arm.body);
                env.pop();
                return result;
            }
        }
    }
    Err(anyhow!(
        "non-exhaustive match during staging (scrutinee = {n})"
    ))
}

// ── Object-level unstager ─────────────────────────────────────────────────────

/// Unstage an object-level `term`, eliminating all `Splice` nodes.
fn unstage_obj<'out>(
    arena: &'out Bump,
    globals: &Globals<'out>,
    env: &mut Env<'out>,
    term: &'out Term<'out>,
) -> Result<&'out Term<'out>> {
    match term {
        // ── Variable ─────────────────────────────────────────────────────────
        Term::Var(lvl) => match env.get(*lvl) {
            Binding::Obj(out_lvl) => Ok(arena.alloc(Term::Var(*out_lvl))),
            Binding::Meta(MetaVal::Code(obj)) => Ok(obj),
            Binding::Meta(MetaVal::Lit(_)) => unreachable!(
                "integer meta variable at level {} referenced in object context \
                 (typechecker invariant)",
                lvl.0
            ),
            Binding::Meta(MetaVal::Closure { .. }) => unreachable!(
                "closure meta variable at level {} referenced in object context \
                 (typechecker invariant)",
                lvl.0
            ),
            Binding::Meta(MetaVal::Ty) => unreachable!(
                "type meta variable at level {} referenced in object context \
                 (typechecker invariant)",
                lvl.0
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

        // ── PrimApp ──────────────────────────────────────────────────────────
        Term::PrimApp(app) => {
            let staged_args: &'out [&'out Term<'out>] = arena.alloc_slice_try_fill_iter(
                app.args
                    .iter()
                    .map(|arg| unstage_obj(arena, globals, env, arg)),
            )?;
            Ok(arena.alloc(Term::new_prim_app(app.prim, staged_args)))
        }

        // ── FunApp (in object terms) ─────────────────────────────────────────
        Term::FunApp(app) => {
            let staged_func = unstage_obj(arena, globals, env, app.func)?;
            let staged_arg = unstage_obj(arena, globals, env, app.arg)?;
            Ok(arena.alloc(Term::FunApp(FunApp {
                func: staged_func,
                arg: staged_arg,
            })))
        }

        // ── Splice: $(t) — the key staging step ──────────────────────────────
        Term::Splice(inner) => {
            let meta_val = eval_meta(arena, globals, env, inner)?;
            match meta_val {
                MetaVal::Code(obj_term) => Ok(obj_term),
                MetaVal::Lit(_) | MetaVal::Ty | MetaVal::Closure { .. } => {
                    unreachable!("splice evaluated to non-code value (typechecker invariant)")
                }
            }
        }

        // ── Let binding ──────────────────────────────────────────────────────
        Term::Let(let_) => {
            let staged_ty = unstage_obj(arena, globals, env, let_.ty)?;
            let staged_expr = unstage_obj(arena, globals, env, let_.expr)?;
            env.push_obj();
            let staged_body = unstage_obj(arena, globals, env, let_.body);
            env.pop();
            Ok(arena.alloc(Term::new_let(
                arena.alloc_str(let_.name),
                staged_ty,
                staged_expr,
                staged_body?,
            )))
        }

        // ── Match ────────────────────────────────────────────────────────────
        Term::Match(match_) => {
            let staged_scrutinee = unstage_obj(arena, globals, env, match_.scrutinee)?;
            let staged_arms: &'out [Arm<'out>] =
                arena.alloc_slice_try_fill_iter(match_.arms.iter().map(|arm| -> Result<_> {
                    let staged_pat = match &arm.pat {
                        Pat::Lit(n) => Pat::Lit(*n),
                        Pat::Bind(name) => Pat::Bind(arena.alloc_str(name)),
                        Pat::Wildcard => Pat::Wildcard,
                    };
                    let has_binding = arm.pat.bound_name().is_some();
                    if has_binding {
                        env.push_obj();
                    }
                    let staged_body = unstage_obj(arena, globals, env, arm.body);
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
pub fn unstage_program<'out>(
    arena: &'out Bump,
    program: &'out Program<'out>,
) -> Result<Program<'out>> {
    let globals: Globals<'out> = program
        .functions
        .iter()
        .map(|f| {
            (
                f.name,
                GlobalDef {
                    sig: &f.sig,
                    body: f.body,
                },
            )
        })
        .collect();

    let staged_fns: Vec<Function<'out>> = program
        .functions
        .iter()
        .filter(|f| f.sig.phase == Phase::Object)
        .map(|f| -> Result<_> {
            let mut env = Env::new(Lvl::new(0));

            let staged_params = arena.alloc_slice_try_fill_iter(f.sig.params.iter().map(
                |(n, ty)| -> Result<(&'out str, &'out Term<'out>)> {
                    let staged_ty = unstage_obj(arena, &globals, &mut env, ty)?;
                    env.push_obj();
                    Ok((arena.alloc_str(n), staged_ty))
                },
            ))?;

            let staged_ret_ty = unstage_obj(arena, &globals, &mut env, f.sig.ret_ty)?;

            let staged_body = unstage_obj(arena, &globals, &mut env, f.body)?;

            Ok(Function {
                name: Name::new(arena.alloc_str(f.name.as_str())),
                sig: FunSig {
                    params: staged_params,
                    ret_ty: staged_ret_ty,
                    phase: f.sig.phase,
                },
                body: staged_body,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    let functions = arena.alloc_slice_fill_iter(staged_fns);
    Ok(Program { functions })
}
