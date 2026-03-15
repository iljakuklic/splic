use std::collections::HashMap;

use anyhow::{anyhow, Result};
use bumpalo::Bump;

use crate::core::{Arm, FunSig, Function, Head, IntType, IntWidth, Lvl, Pat, Prim, Program, Term};
use crate::parser::ast::Phase;

// ── Value types ───────────────────────────────────────────────────────────────

/// A value produced by meta-level evaluation.
///
/// In this substitution-based prototype there are only two kinds of meta
/// values: integer literals and quoted object-level code.  Meta-level
/// lambdas / closures are not yet supported (the current surface language
/// has no meta-level lambda syntax; only top-level `fn` definitions).
#[derive(Clone, Debug)]
enum MetaVal<'a> {
    /// A concrete integer value computed at meta (compile) time.
    VLit(u64),
    /// Quoted object-level code: the result of evaluating `#(t)` or of
    /// wrapping a literal via `Embed`.  The inner term is a splice-free
    /// object term produced by `unstage_obj`.
    VCode(&'a Term<'a>),
}

// ── Environment ───────────────────────────────────────────────────────────────

/// A binding stored in the evaluation environment, indexed by De Bruijn level.
#[derive(Clone, Debug)]
enum Binding<'a> {
    /// A meta-level variable bound to a concrete `MetaVal`.
    Meta(MetaVal<'a>),
    /// An object-level variable.  Object variables are opaque during
    /// meta-level evaluation and remain as `Var(lvl)` in the output.
    /// The Lvl inside signifies the level in the generated output
    /// rather than in the original program where bindings for the
    /// object level and meta level may be interwoven.
    Obj(Lvl),
}

/// Evaluation environment: a stack of bindings indexed by De Bruijn level.
///
/// Level 0 is the outermost binding (first function parameter); new bindings
/// are pushed onto the end and accessed by their index.
#[derive(Debug)]
struct Env<'a> {
    bindings: Vec<Binding<'a>>,
    obj_next: Lvl,
}

impl<'a> Env<'a> {
    fn new(obj_next: Lvl) -> Self {
        Env {
            bindings: Vec::new(),
            obj_next,
        }
    }

    /// Look up the binding at level `lvl`.
    fn get(&self, lvl: Lvl) -> &Binding<'a> {
        &self.bindings[lvl.0]
    }

    /// Push an object-level binding.  Assigns the next consecutive object-level
    /// De Bruijn level and advances `obj_next`.
    fn push_obj(&mut self) {
        let lvl = self.obj_next;
        self.obj_next = lvl.succ();
        self.bindings.push(Binding::Obj(lvl));
    }

    /// Push a meta-level binding bound to the given value.
    fn push_meta(&mut self, val: MetaVal<'a>) {
        self.bindings.push(Binding::Meta(val));
    }

    /// Pop the last binding (used to restore the environment after a let / arm).
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

type Globals<'a> = HashMap<&'a str, GlobalDef<'a>>;

// ── Meta-level evaluator ──────────────────────────────────────────────────────

/// Evaluate a meta-level `term` to a `MetaVal`.
///
/// `env` maps De Bruijn levels to their current values.  `globals` provides
/// the definitions of all top-level functions.  `arena` is used when
/// allocating object terms inside `VCode` values (via `unstage_obj`).
///
/// Invariants enforced by the typechecker (violations panic via `unreachable!`):
/// - `Splice` nodes never appear at meta level.
/// - `Lift` and type-level `Prim` nodes never appear in term positions.
/// - Object variables (`Binding::Obj`) are never referenced at meta level.
fn eval_meta<'a>(
    arena: &'a Bump,
    globals: &Globals<'a>,
    env: &mut Env<'a>,
    term: &'a Term<'a>,
) -> Result<MetaVal<'a>> {
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
        Term::Lit(n) => Ok(MetaVal::VLit(*n)),

        // ── Application ──────────────────────────────────────────────────────
        Term::App { head, args } => eval_meta_app(arena, globals, env, head, args),

        // ── Quote: #(t) ───────────────────────────────────────────────────────
        // Unstage the enclosed object term (eliminating any splices inside it)
        // and wrap the result as object code.
        Term::Quote(inner) => {
            let obj_term = unstage_obj(arena, globals, env, inner)?;
            Ok(MetaVal::VCode(obj_term))
        }

        // ── Let binding ───────────────────────────────────────────────────────
        Term::Let { expr, body, .. } => {
            let val = eval_meta(arena, globals, env, expr)?;
            env.push_meta(val);
            let result = eval_meta(arena, globals, env, body);
            env.pop();
            result
        }

        // ── Match ─────────────────────────────────────────────────────────────
        Term::Match { scrutinee, arms } => {
            let scrut_val = eval_meta(arena, globals, env, scrutinee)?;
            let n = match scrut_val {
                MetaVal::VLit(n) => n,
                MetaVal::VCode(_) => unreachable!(
                    "cannot match on object code at meta level (typechecker invariant)"
                ),
            };
            eval_meta_match(arena, globals, env, n, arms)
        }

        // ── Unreachable in well-typed meta terms ──────────────────────────────
        Term::Splice(_) => unreachable!("Splice in meta context (typechecker invariant)"),
        Term::Lift(_) | Term::Prim(_) => {
            unreachable!("type-level term in evaluation position (typechecker invariant)")
        }
    }
}

/// Evaluate a function application at meta level.
fn eval_meta_app<'a>(
    arena: &'a Bump,
    globals: &Globals<'a>,
    env: &mut Env<'a>,
    head: &'a Head<'a>,
    args: &'a [&'a Term<'a>],
) -> Result<MetaVal<'a>> {
    match head {
        // ── Global function call ──────────────────────────────────────────────
        Head::Global(name) => {
            let def = globals
                .get(name)
                .unwrap_or_else(|| panic!("unknown global `{name}` during staging"));

            assert_eq!(
                def.sig.phase,
                Phase::Meta,
                "object-phase function `{name}` called in meta context during staging"
            );

            // Evaluate each argument in the *caller's* environment.
            let mut arg_vals: Vec<MetaVal<'a>> = Vec::with_capacity(args.len());
            for arg in args {
                arg_vals.push(eval_meta(arena, globals, env, arg)?);
            }

            // Build a fresh environment for the callee: one binding per parameter.
            let mut callee_env = Env::new(env.obj_next);
            for val in arg_vals {
                callee_env.push_meta(val);
            }

            eval_meta(arena, globals, &mut callee_env, def.body)
        }

        // ── Primitive operations ──────────────────────────────────────────────
        Head::Prim(prim) => eval_meta_prim(arena, globals, env, prim, args),
    }
}

/// Evaluate a primitive operation at meta level.
fn eval_meta_prim<'a>(
    arena: &'a Bump,
    globals: &Globals<'a>,
    env: &mut Env<'a>,
    prim: &Prim,
    args: &'a [&'a Term<'a>],
) -> Result<MetaVal<'a>> {
    // Evaluate args[i] and extract its integer value.
    // Panics if the value is `VCode` — the typechecker guarantees integer operands.
    let eval_lit = |arena: &'a Bump, globals: &Globals<'a>, env: &mut Env<'a>, i: usize| {
        eval_meta(arena, globals, env, args[i]).map(|v| match v {
            MetaVal::VLit(n) => n,
            MetaVal::VCode(_) => unreachable!(
                "expected integer meta value for primitive operand {i}, got code (typechecker invariant)"
            ),
        })
    };

    match prim {
        // ── Arithmetic ────────────────────────────────────────────────────────
        Prim::Add(_) => {
            let a = eval_lit(arena, globals, env, 0)?;
            let b = eval_lit(arena, globals, env, 1)?;
            Ok(MetaVal::VLit(a.wrapping_add(b)))
        }
        Prim::Sub(_) => {
            let a = eval_lit(arena, globals, env, 0)?;
            let b = eval_lit(arena, globals, env, 1)?;
            Ok(MetaVal::VLit(a.wrapping_sub(b)))
        }
        Prim::Mul(_) => {
            let a = eval_lit(arena, globals, env, 0)?;
            let b = eval_lit(arena, globals, env, 1)?;
            Ok(MetaVal::VLit(a.wrapping_mul(b)))
        }
        Prim::Div(_) => {
            let a = eval_lit(arena, globals, env, 0)?;
            let b = eval_lit(arena, globals, env, 1)?;
            if b == 0 {
                return Err(anyhow!("division by zero during staging"));
            }
            Ok(MetaVal::VLit(a / b))
        }

        // ── Bitwise ───────────────────────────────────────────────────────────
        Prim::BitAnd(_) => {
            let a = eval_lit(arena, globals, env, 0)?;
            let b = eval_lit(arena, globals, env, 1)?;
            Ok(MetaVal::VLit(a & b))
        }
        Prim::BitOr(_) => {
            let a = eval_lit(arena, globals, env, 0)?;
            let b = eval_lit(arena, globals, env, 1)?;
            Ok(MetaVal::VLit(a | b))
        }
        Prim::BitNot(IntType { width, .. }) => {
            let a = eval_lit(arena, globals, env, 0)?;
            Ok(MetaVal::VLit(mask_to_width(*width, !a)))
        }

        // ── Comparison ────────────────────────────────────────────────────────
        Prim::Eq(_) => {
            let a = eval_lit(arena, globals, env, 0)?;
            let b = eval_lit(arena, globals, env, 1)?;
            Ok(MetaVal::VLit(u64::from(a == b)))
        }
        Prim::Ne(_) => {
            let a = eval_lit(arena, globals, env, 0)?;
            let b = eval_lit(arena, globals, env, 1)?;
            Ok(MetaVal::VLit(u64::from(a != b)))
        }
        Prim::Lt(_) => {
            let a = eval_lit(arena, globals, env, 0)?;
            let b = eval_lit(arena, globals, env, 1)?;
            Ok(MetaVal::VLit(u64::from(a < b)))
        }
        Prim::Gt(_) => {
            let a = eval_lit(arena, globals, env, 0)?;
            let b = eval_lit(arena, globals, env, 1)?;
            Ok(MetaVal::VLit(u64::from(a > b)))
        }
        Prim::Le(_) => {
            let a = eval_lit(arena, globals, env, 0)?;
            let b = eval_lit(arena, globals, env, 1)?;
            Ok(MetaVal::VLit(u64::from(a <= b)))
        }
        Prim::Ge(_) => {
            let a = eval_lit(arena, globals, env, 0)?;
            let b = eval_lit(arena, globals, env, 1)?;
            Ok(MetaVal::VLit(u64::from(a >= b)))
        }

        // ── Embed: meta integer → object code ─────────────────────────────────
        // `Embed(w)` applied to a meta integer `n` produces object-level code
        // consisting of the literal `n`.  This is how a compile-time integer
        // constant is embedded into the generated object program.
        Prim::Embed(_) => {
            let n = eval_lit(arena, globals, env, 0)?;
            let lit_term = arena.alloc(Term::Lit(n));
            Ok(MetaVal::VCode(lit_term))
        }

        // ── Type-level prims are unreachable ──────────────────────────────────
        Prim::IntTy(_) | Prim::U(_) => {
            unreachable!("type-level primitive in evaluation position (typechecker invariant)")
        }
    }
}

/// Mask `val` to the bit-width of `width`.
fn mask_to_width(width: IntWidth, val: u64) -> u64 {
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
///
/// `n` is the already-evaluated scrutinee value.
/// Arms are checked in order; the first matching arm wins.
fn eval_meta_match<'a>(
    arena: &'a Bump,
    globals: &Globals<'a>,
    env: &mut Env<'a>,
    n: u64,
    arms: &'a [Arm<'a>],
) -> Result<MetaVal<'a>> {
    for arm in arms {
        match &arm.pat {
            Pat::Lit(m) => {
                if n == *m {
                    return eval_meta(arena, globals, env, arm.body);
                }
            }
            Pat::Bind(_) | Pat::Wildcard => {
                // Catch-all: bind the scrutinee value and evaluate the body.
                env.push_meta(MetaVal::VLit(n));
                let result = eval_meta(arena, globals, env, arm.body);
                env.pop();
                return result;
            }
        }
    }
    // The typechecker enforces exhaustiveness, so this should not be reachable
    // for well-typed programs.  It can happen if the meta computation produces
    // a value outside the covered range (e.g. a u64 with no wildcard arm), which
    // is a user-visible runtime staging error rather than an internal bug.
    Err(anyhow!(
        "non-exhaustive match during staging (scrutinee = {n})"
    ))
}

// ── Object-level unstager ─────────────────────────────────────────────────────

/// Unstage an object-level `term`, eliminating all `Splice` nodes.
///
/// Object variables (`Var`), operations (`App`), `Let`, and `Match` are left
/// structurally intact; only `Splice` nodes are reduced by running the
/// enclosed meta computation.
///
/// `env` is shared with the meta evaluator so that meta variables that are in
/// scope at a splice point (e.g. from an enclosing `Quote`) remain accessible.
fn unstage_obj<'a>(
    arena: &'a Bump,
    globals: &Globals<'a>,
    env: &mut Env<'a>,
    term: &'a Term<'a>,
) -> Result<&'a Term<'a>> {
    match term {
        // ── Variable ─────────────────────────────────────────────────────────
        Term::Var(lvl) => match env.get(*lvl) {
            // A plain object variable (e.g. a `code fn` parameter) passes
            // through as-is — it will be a free variable in the output.
            Binding::Obj(out_lvl) => Ok(arena.alloc(Term::Var(*out_lvl))),
            // A meta variable of type `[[T]]` is referenced inside a quoted
            // object term.  Its value is object code.  `VCode` is always
            // fully staged (produced by `unstage_obj` at quote time), so we
            // return it directly without recursing — that would be unsound
            // because the levels inside the VCode term are relative to the
            // environment at the *quoting site*, not the current env.
            // This implements the ∼⟨t⟩ ≡ t definitional equality.
            Binding::Meta(MetaVal::VCode(obj)) => Ok(obj),
            Binding::Meta(MetaVal::VLit(_)) => unreachable!(
                "integer meta variable at level {} referenced in object context \
                 (typechecker invariant: only [[T]]-typed meta vars can appear in object terms)",
                lvl.0
            ),
        },

        // ── Literal ──────────────────────────────────────────────────────────
        Term::Lit(n) => Ok(arena.alloc(Term::Lit(*n))),

        // ── Primitive ────────────────────────────────────────────────────────
        Term::Prim(p) => Ok(arena.alloc(Term::Prim(*p))),

        // ── Application ──────────────────────────────────────────────────────
        Term::App { head, args } => {
            let staged_args: &'a [&'a Term<'a>] = arena.alloc_slice_try_fill_iter(
                args.iter().map(|arg| unstage_obj(arena, globals, env, arg)),
            )?;
            Ok(arena.alloc(Term::App {
                head: head.clone(),
                args: staged_args,
            }))
        }

        // ── Splice: $(t) — the key staging step ───────────────────────────────
        // Evaluate the meta term `t` to a `VCode(obj)`.  `VCode` values are
        // always fully staged (produced by `unstage_obj` at quote time), so
        // we return the inner term directly.
        Term::Splice(inner) => {
            let meta_val = eval_meta(arena, globals, env, inner)?;
            match meta_val {
                MetaVal::VCode(obj_term) => Ok(obj_term),
                MetaVal::VLit(_) => unreachable!(
                    "splice evaluated to an integer literal (typechecker invariant: \
                     splice argument must have type [[T]])"
                ),
            }
        }

        // ── Let binding ───────────────────────────────────────────────────────
        Term::Let {
            name,
            ty,
            expr,
            body,
        } => {
            let staged_expr = unstage_obj(arena, globals, env, expr)?;
            // Push an object binding so that subsequent Var references by
            // De Bruijn level resolve to the correct slot.
            env.push_obj();
            let staged_body = unstage_obj(arena, globals, env, body);
            env.pop();
            Ok(arena.alloc(Term::Let {
                name,
                ty,
                expr: staged_expr,
                body: staged_body?,
            }))
        }

        // ── Match ─────────────────────────────────────────────────────────────
        Term::Match { scrutinee, arms } => {
            let staged_scrutinee = unstage_obj(arena, globals, env, scrutinee)?;
            let staged_arms: &'a [Arm<'a>] =
                arena.alloc_slice_try_fill_iter(arms.iter().map(|arm| -> Result<_> {
                    let has_binding = arm.pat.bound_name().is_some();
                    if has_binding {
                        env.push_obj();
                    }
                    let staged_body = unstage_obj(arena, globals, env, arm.body);
                    if has_binding {
                        env.pop();
                    }
                    Ok(Arm {
                        pat: arm.pat.clone(),
                        body: staged_body?,
                    })
                }))?;
            Ok(arena.alloc(Term::Match {
                scrutinee: staged_scrutinee,
                arms: staged_arms,
            }))
        }

        // ── Unreachable in well-typed object terms ────────────────────────────
        Term::Quote(_) => unreachable!("Quote in object context (typechecker invariant)"),
        Term::Lift(_) => unreachable!("Lift in object context (typechecker invariant)"),
    }
}

// ── Public entry point ────────────────────────────────────────────────────────

/// Unstage an elaborated program, eliminating all meta-level functions and
/// splices to produce a splice-free object-level program.
///
/// The output `Program` contains only `Phase::Object` functions.  All
/// `Phase::Meta` functions are erased (they served only as compile-time
/// helpers).  Every `Splice` node in object-function bodies is replaced by
/// the object code it produces when the enclosing meta computation runs.
///
/// # Errors
///
/// Returns an error for genuine runtime staging errors: division by zero,
/// or a non-exhaustive match on a value not covered by any arm.
pub fn unstage_program<'a>(arena: &'a Bump, program: &'a Program<'a>) -> Result<Program<'a>> {
    // Build the globals table from all functions in the program.
    let globals: Globals<'a> = program
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

    // Unstage each object-level function; discard meta-level functions.
    let staged_fns: Vec<Function<'a>> = program
        .functions
        .iter()
        .filter(|f| f.sig.phase == Phase::Object)
        .map(|f| -> Result<_> {
            // Build an initial environment: one Obj binding per parameter,
            // so that parameter De Bruijn levels are correct.
            let mut env = Env::new(Lvl::new(0));
            for _ in f.sig.params {
                env.push_obj();
            }

            let staged_body = unstage_obj(arena, &globals, &mut env, f.body)?;

            Ok(Function {
                name: f.name,
                sig: FunSig {
                    params: f.sig.params,
                    ret_ty: f.sig.ret_ty,
                    phase: f.sig.phase,
                },
                body: staged_body,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    let functions = arena.alloc_slice_fill_iter(staged_fns);
    Ok(Program { functions })
}

#[cfg(test)]
mod test;
