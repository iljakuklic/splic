use anyhow::{Context as _, Result, anyhow, bail, ensure};

use crate::core::{self, IntType, IntWidth, Lam, Lvl, Phase, Pi, Prim, value};
use crate::parser::ast;

use super::{Ctx, builtin_prim_ty, types_equal_val, value_type_universe_ctx};

pub fn infer<'src, 'core>(
    ctx: &mut Ctx<'core, '_>,
    phase: Phase,
    term: &'src ast::Term<'src>,
) -> Result<&'core core::Term<'core>> {
    match term {
        // ------------------------------------------------------------------ Var
        // Look up the name in locals; return its index and type.
        ast::Term::Var(name) => {
            // First check if it's a built-in type name — those are inferable too.
            if let Some(term) = builtin_prim_ty(name, phase) {
                // Phase check: U(Object) (VmType) is only valid in a meta-phase context.
                if let core::Term::Prim(Prim::U(u_phase)) = term {
                    ensure!(
                        *u_phase == phase,
                        "`{name}` is a {u_phase}-phase type, \
                         not valid in a {phase}-phase context"
                    );
                }
                return Ok(term);
            }
            // Check locals.
            if let Some((ix, _)) = ctx.lookup_local(name) {
                return Ok(ctx.alloc(core::Term::Var(ix)));
            }
            // Check globals — bare reference without call, produces Global term.
            if ctx.globals.contains_key(name) {
                let name = core::Name::new(ctx.arena.alloc_str(name.as_str()));
                return Ok(ctx.alloc(core::Term::Global(name)));
            }
            Err(anyhow!("unbound variable `{name}`"))
        }

        // ------------------------------------------------------------------ Lit
        // Literals have no intrinsic type — they are check-only.
        ast::Term::Lit(_) => Err(anyhow!(
            "cannot infer type of a literal; add a type annotation"
        )),

        // ------------------------------------------------------------------ App { Global or local }
        // Function calls: look up callee, elaborate as curried FunApp chain.
        ast::Term::App {
            func: ast::FunName::Term(func_term),
            args,
        } => {
            // Elaborate the callee.
            let callee = infer(ctx, phase, func_term)?;

            // For globals: verify phase and arity using the raw Pi term.
            // Non-globals: Pi depth is indistinguishable from nested fn types at value level,
            // so we skip the arity pre-check and let the arg loop catch mismatches.
            if let core::Term::Global(gname) = callee {
                let (pi_phase, pi_param_count) = callee_pi_info(ctx, callee)?;
                ensure!(
                    pi_phase == phase,
                    "function `{gname}` is a {pi_phase}-phase function, but called in {phase}-phase context",
                );
                ensure!(
                    args.len() == pi_param_count,
                    "wrong number of arguments: callee expects {pi_param_count}, got {}",
                    args.len()
                );
            }

            // Get the starting Pi value for arg checking.
            // For globals: evaluate the Pi term in empty env.
            // For locals: use val_type_of (Value::Pi).
            let mut pi_val = callee_pi_val(ctx, callee);
            let mut core_args: Vec<&'core core::Term<'core>> = Vec::with_capacity(args.len());
            for (i, arg) in args.iter().enumerate() {
                let vpi = match pi_val {
                    value::Value::Pi(vpi) => vpi,
                    _ => bail!("too many arguments at argument {i}"),
                };
                // Check the arg against the domain type.
                let core_arg = check_val(ctx, phase, arg, (*vpi.domain).clone())
                    .with_context(|| format!("in argument {i} of function call"))?;
                let arg_val = ctx.eval(core_arg);
                core_args.push(core_arg);
                // Advance Pi to the next type by applying closure to arg.
                pi_val = value::inst(ctx.arena, &vpi.closure, arg_val);
            }

            let args_slice = ctx.alloc_slice(core_args);
            Ok(ctx.alloc(core::Term::new_app(callee, args_slice)))
        }

        // ------------------------------------------------------------------ App { Prim (BinOp/UnOp) }
        // Comparison ops are inferable: they always return u1.
        ast::Term::App {
            func: ast::FunName::BinOp(op),
            args,
        } if matches!(
            op,
            ast::BinOp::Eq
                | ast::BinOp::Ne
                | ast::BinOp::Lt
                | ast::BinOp::Gt
                | ast::BinOp::Le
                | ast::BinOp::Ge
        ) =>
        {
            use ast::BinOp;
            let [lhs, rhs] = args else {
                bail!("binary operation expects exactly 2 arguments")
            };

            let core_arg0 = infer(ctx, phase, lhs)?;
            let operand_ty_val = ctx.val_type_of(core_arg0);
            let operand_ty_term = ctx.quote_val(&operand_ty_val);
            let core_arg1 = check(ctx, phase, rhs, operand_ty_term)?;
            let op_int_ty = match &operand_ty_val {
                value::Value::Prim(Prim::IntTy(it)) => *it,
                _ => bail!("comparison operands must be integers"),
            };
            let prim = match op {
                BinOp::Eq => Prim::Eq(op_int_ty),
                BinOp::Ne => Prim::Ne(op_int_ty),
                BinOp::Lt => Prim::Lt(op_int_ty),
                BinOp::Gt => Prim::Gt(op_int_ty),
                BinOp::Le => Prim::Le(op_int_ty),
                BinOp::Ge => Prim::Ge(op_int_ty),
                BinOp::Add
                | BinOp::Sub
                | BinOp::Mul
                | BinOp::Div
                | BinOp::BitAnd
                | BinOp::BitOr => unreachable!(),
            };
            let core_args = ctx.alloc_slice([core_arg0, core_arg1]);
            Ok(ctx.alloc(core::Term::new_app(
                ctx.alloc(core::Term::Prim(prim)),
                core_args,
            )))
        }
        ast::Term::App {
            func: ast::FunName::BinOp(_) | ast::FunName::UnOp(_),
            ..
        } => Err(anyhow!(
            "cannot infer type of a primitive operation; add a type annotation"
        )),

        // ------------------------------------------------------------------ Pi
        // Function type expression: elaborate each param type, push locals, elaborate body type.
        ast::Term::Pi { params, ret_ty } => {
            ensure!(
                phase == Phase::Meta,
                "function types are only valid in meta-phase context"
            );
            let depth_before = ctx.depth();

            let mut elaborated_params: Vec<(&'core core::Name, &'core core::Term<'core>)> =
                Vec::new();
            for p in *params {
                let param_name = core::Name::new(ctx.arena.alloc_str(p.name.as_str()));
                let param_ty = infer(ctx, Phase::Meta, p.ty)?;
                ensure!(
                    value_type_universe_ctx(ctx, &ctx.eval(param_ty)).is_some(),
                    "parameter type must be a type"
                );
                elaborated_params.push((param_name, param_ty));
                ctx.push_local(param_name, param_ty);
            }

            let core_ret_ty = infer(ctx, Phase::Meta, ret_ty)?;
            ensure!(
                value_type_universe_ctx(ctx, &ctx.eval(core_ret_ty)).is_some(),
                "return type must be a type"
            );

            for _ in &elaborated_params {
                ctx.pop_local();
            }
            assert_eq!(ctx.depth(), depth_before, "Pi elaboration leaked locals");
            let params_slice = ctx.alloc_slice(elaborated_params);
            Ok(ctx.alloc(core::Term::Pi(Pi {
                params: params_slice,
                body_ty: core_ret_ty,
                phase: Phase::Meta,
            })))
        }

        // ------------------------------------------------------------------ Lam (infer mode)
        // Lambda with mandatory type annotations — inferable.
        ast::Term::Lam { params, body } => {
            ensure!(
                phase == Phase::Meta,
                "lambdas are only valid in meta-phase context"
            );

            let depth_before = ctx.depth();
            let mut elaborated_params: Vec<(&'core core::Name, &'core core::Term<'core>)> =
                Vec::new();

            for p in *params {
                let param_name = core::Name::new(ctx.arena.alloc_str(p.name.as_str()));
                let param_ty = infer(ctx, Phase::Meta, p.ty)?;
                elaborated_params.push((param_name, param_ty));
                ctx.push_local(param_name, param_ty);
            }

            let core_body = infer(ctx, phase, body)?;

            for _ in &elaborated_params {
                ctx.pop_local();
            }
            assert_eq!(ctx.depth(), depth_before, "Lam elaboration leaked locals");
            let params_slice = ctx.alloc_slice(elaborated_params);
            Ok(ctx.alloc(core::Term::Lam(Lam {
                params: params_slice,
                body: core_body,
            })))
        }

        // ------------------------------------------------------------------ Lift
        ast::Term::Lift(inner) => {
            ensure!(
                phase == Phase::Meta,
                "`[[...]]` is only valid in a meta-phase context"
            );
            let core_inner = infer(ctx, Phase::Object, inner)?;
            let inner_ty_val = ctx.val_type_of(core_inner);
            let is_vm_type = matches!(&inner_ty_val, value::Value::Prim(Prim::U(Phase::Object)));
            ensure!(is_vm_type, "argument of `[[...]]` must be an object type");
            Ok(ctx.alloc(core::Term::Lift(core_inner)))
        }

        // ------------------------------------------------------------------ Quote
        ast::Term::Quote(inner) => {
            ensure!(
                phase == Phase::Meta,
                "`#(...)` is only valid in a meta-phase context"
            );
            let core_inner = infer(ctx, Phase::Object, inner)?;
            Ok(ctx.alloc(core::Term::Quote(core_inner)))
        }

        // ------------------------------------------------------------------ Splice
        ast::Term::Splice(inner) => {
            ensure!(
                phase == Phase::Object,
                "`$(...)` is only valid in an object-phase context"
            );
            let core_inner = infer(ctx, Phase::Meta, inner)?;
            let inner_ty_val = ctx.val_type_of(core_inner);
            match &inner_ty_val {
                value::Value::Lift(_) => Ok(ctx.alloc(core::Term::Splice(core_inner))),
                value::Value::Prim(Prim::IntTy(IntType {
                    width,
                    phase: Phase::Meta,
                })) => {
                    let embedded = ctx.alloc(core::Term::new_app(
                        ctx.alloc(core::Term::Prim(Prim::Embed(*width))),
                        ctx.alloc_slice([core_inner]),
                    ));
                    Ok(ctx.alloc(core::Term::Splice(embedded)))
                }
                _ => Err(anyhow!(
                    "argument of `$(...)` must have a lifted type `[[T]]` or be a meta-level integer"
                )),
            }
        }

        // ------------------------------------------------------------------ Block (Let*)
        ast::Term::Block { stmts, expr } => {
            let depth_before = ctx.depth();
            let result = infer_block(ctx, phase, stmts, expr);
            assert_eq!(ctx.depth(), depth_before, "infer_block leaked locals");
            result
        }

        // ------------------------------------------------------------------ Match
        ast::Term::Match { .. } => Err(anyhow!(
            "cannot infer type of match expression; add a type annotation or use in a \
             checked position"
        )),
    }
}

/// Return the Pi phase and parameter count for a callee.
///
/// For a `Global`, reads the raw Pi term from the globals table (a closed term).
/// For any other callee, peels `Value::Pi` layers from `val_type_of`.
fn callee_pi_info(ctx: &Ctx<'_, '_>, callee: &core::Term<'_>) -> Result<(Phase, usize)> {
    match callee {
        core::Term::Global(name) => {
            let pi = ctx
                .globals
                .get(name)
                .copied()
                .ok_or_else(|| anyhow!("unknown global `{name}`"))?;
            Ok((pi.phase, pi.params.len()))
        }
        _ => {
            let mut ty = ctx.val_type_of(callee);
            let mut count = 0usize;
            let mut phase_opt: Option<Phase> = None;
            while let value::Value::Pi(vpi) = ty {
                if phase_opt.is_none() {
                    phase_opt = Some(vpi.phase);
                }
                count += 1;
                // Advance with a fresh rigid to get the next Pi layer.
                let fresh = value::Value::Rigid(Lvl(ctx.depth() + count - 1));
                ty = value::inst(ctx.arena, &vpi.closure, fresh);
            }
            // If no Pi layers were found (count == 0), the callee's type reduces to
            // a non-Pi value. In this design fn() -> T ≅ T, so zero-arg calls are
            // valid for any callee. Phase is unused for non-global callees.
            let phase = phase_opt.unwrap_or(Phase::Meta);
            Ok((phase, count))
        }
    }
}

/// Return the starting Pi `Value` for argument checking.
///
/// For a `Global`, evaluates the closed Pi term in the current environment.
/// For any other callee, returns `val_type_of` directly (already a `Value::Pi`).
fn callee_pi_val<'core>(
    ctx: &Ctx<'core, '_>,
    callee: &'core core::Term<'core>,
) -> value::Value<'core> {
    match callee {
        core::Term::Global(name) => {
            let pi = ctx
                .globals
                .get(name)
                .copied()
                .expect("callee_pi_val called with unknown global (invariant)");
            // Global Pi terms are closed (elaborated in empty context) — safe to eval in current env.
            value::eval_pi(ctx.arena, &[], pi)
        }
        _ => ctx.val_type_of(callee),
    }
}

/// Check exhaustiveness of `arms` given the scrutinee type `scrut_ty`.
fn check_exhaustiveness(scrut_ty: &value::Value<'_>, arms: &[ast::MatchArm<'_>]) -> Result<()> {
    let mut covered_lits: Option<Vec<bool>> = match scrut_ty {
        value::Value::Prim(Prim::IntTy(ty)) => match ty.width {
            IntWidth::U0 => Some(vec![false; 1]),
            IntWidth::U1 => Some(vec![false; 2]),
            IntWidth::U8 => Some(vec![false; 256]),
            IntWidth::U16 | IntWidth::U32 | IntWidth::U64 => None,
        },
        _ => None,
    };
    let mut has_catch_all = false;

    for arm in arms {
        match &arm.pat {
            ast::Pat::Name(_) => {
                has_catch_all = true;
            }
            ast::Pat::Lit(n) => {
                if let Some(ref mut bits) = covered_lits {
                    let bit = bits
                        .get_mut(usize::try_from(*n)?)
                        .context("Pattern literal out of range")?;
                    *bit = true;
                }
            }
        }
    }

    let fully_covered = covered_lits.is_some_and(|bits| bits.iter().all(|&b| b));
    ensure!(
        has_catch_all || fully_covered,
        "match expression is not exhaustive: no wildcard or bind-all arm"
    );
    Ok(())
}

/// Elaborate a match pattern into a core pattern.
fn elaborate_pat<'core>(ctx: &Ctx<'core, '_>, pat: &ast::Pat<'_>) -> core::Pat<'core> {
    match pat {
        ast::Pat::Lit(n) => core::Pat::Lit(*n),
        ast::Pat::Name(name) => {
            let s = name.as_str();
            if s == "_" {
                core::Pat::Wildcard
            } else {
                let bound = core::Name::new(ctx.arena.alloc_str(s));
                core::Pat::Bind(bound)
            }
        }
    }
}

/// Elaborate a single `let` binding.
fn elaborate_let<'src, 'core, T, F, G, W>(
    ctx: &mut Ctx<'core, '_>,
    phase: Phase,
    stmt: &'src ast::Let<'src>,
    cont: F,
    body_of: G,
    wrap: W,
) -> Result<T>
where
    F: FnOnce(&mut Ctx<'core, '_>) -> Result<T>,
    G: FnOnce(&T) -> &'core core::Term<'core>,
    W: FnOnce(&'core core::Term<'core>, T) -> T,
{
    let (core_expr, bind_ty_val) = if let Some(ann) = stmt.ty {
        let ty = infer(ctx, phase, ann)?;
        let ty_val = ctx.eval(ty);
        let core_e = check_val(ctx, phase, stmt.expr, ty_val.clone())
            .with_context(|| format!("in let binding `{}`", stmt.name.as_str()))?;
        (core_e, ty_val)
    } else {
        let core_e = infer(ctx, phase, stmt.expr)
            .with_context(|| format!("in let binding `{}`", stmt.name.as_str()))?;
        let bind_ty = ctx.val_type_of(core_e);
        (core_e, bind_ty)
    };

    let bind_ty_term = ctx.quote_val(&bind_ty_val);
    // Evaluate the bound expression so dependent references to this binding work correctly.
    let expr_val = ctx.eval(core_expr);
    let bind_name = core::Name::new(ctx.arena.alloc_str(stmt.name.as_str()));
    ctx.push_let_binding(bind_name, bind_ty_val, expr_val);
    let cont_result = cont(ctx);
    ctx.pop_local();
    let cont_result = cont_result?;

    let core_body = body_of(&cont_result);
    let let_term = ctx.alloc(core::Term::new_let(
        bind_name,
        bind_ty_term,
        core_expr,
        core_body,
    ));
    Ok(wrap(let_term, cont_result))
}

/// Elaborate a sequence of `let` bindings followed by a trailing expression (infer mode).
fn infer_block<'src, 'core>(
    ctx: &mut Ctx<'core, '_>,
    phase: Phase,
    stmts: &'src [ast::Let<'src>],
    expr: &'src ast::Term<'src>,
) -> Result<&'core core::Term<'core>> {
    match stmts {
        [] => infer(ctx, phase, expr),
        [first, rest @ ..] => elaborate_let(
            ctx,
            phase,
            first,
            |ctx| infer_block(ctx, phase, rest, expr),
            |body| body,
            |let_term, _body| let_term,
        ),
    }
}

/// Elaborate a sequence of `let` bindings followed by a trailing expression (check mode).
fn check_block_val<'src, 'core>(
    ctx: &mut Ctx<'core, '_>,
    phase: Phase,
    stmts: &'src [ast::Let<'src>],
    expr: &'src ast::Term<'src>,
    expected: value::Value<'core>,
) -> Result<&'core core::Term<'core>> {
    match stmts {
        [] => check_val(ctx, phase, expr, expected),
        [first, rest @ ..] => elaborate_let(
            ctx,
            phase,
            first,
            |ctx| check_block_val(ctx, phase, rest, expr, expected.clone()),
            |body| body,
            |let_term, _body| let_term,
        ),
    }
}

/// Check `term` against `expected` (as a term reference), returning the elaborated core term.
///
/// This is a convenience wrapper for callers that have an expected type as a `&Term`.
pub fn check<'src, 'core>(
    ctx: &mut Ctx<'core, '_>,
    phase: Phase,
    term: &'src ast::Term<'src>,
    expected: &'core core::Term<'core>,
) -> Result<&'core core::Term<'core>> {
    let expected_val = ctx.eval(expected);
    check_val(ctx, phase, term, expected_val)
}

/// Check `term` against `expected` (as a semantic Value), returning the elaborated core term.
pub fn check_val<'src, 'core>(
    ctx: &mut Ctx<'core, '_>,
    phase: Phase,
    term: &'src ast::Term<'src>,
    expected: value::Value<'core>,
) -> Result<&'core core::Term<'core>> {
    // Verify `expected` inhabits the correct universe for the current phase.
    let ty_phase = value_type_universe_ctx(ctx, &expected)
        .expect("expected type passed to `check` is not a well-formed type expression");
    ensure!(
        ty_phase == phase,
        "expected type inhabits the {ty_phase}-phase universe, \
         but elaborating at {phase} phase"
    );
    match term {
        // ------------------------------------------------------------------ Lit
        ast::Term::Lit(n) => match &expected {
            value::Value::Prim(Prim::IntTy(it)) => {
                let width = it.width;
                ensure!(
                    *n <= width.max_value(),
                    "literal `{n}` does not fit in type `{width}`"
                );
                Ok(ctx.alloc(core::Term::Lit(*n, *it)))
            }
            _ => Err(anyhow!("literal `{n}` cannot have a non-integer type")),
        },

        // ------------------------------------------------------------------ App { Prim (BinOp) }
        // Width is resolved from the expected type.
        ast::Term::App {
            func: ast::FunName::BinOp(op),
            args,
        } if !matches!(
            op,
            ast::BinOp::Eq
                | ast::BinOp::Ne
                | ast::BinOp::Lt
                | ast::BinOp::Gt
                | ast::BinOp::Le
                | ast::BinOp::Ge
        ) =>
        {
            let int_ty = match &expected {
                value::Value::Prim(Prim::IntTy(it)) => *it,
                _ => bail!("primitive operation requires an integer type"),
            };

            use ast::BinOp;
            let prim = match op {
                BinOp::Add => Prim::Add(int_ty),
                BinOp::Sub => Prim::Sub(int_ty),
                BinOp::Mul => Prim::Mul(int_ty),
                BinOp::Div => Prim::Div(int_ty),
                BinOp::BitAnd => Prim::BitAnd(int_ty),
                BinOp::BitOr => Prim::BitOr(int_ty),
                BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge => {
                    unreachable!("comparisons are excluded by guard")
                }
            };

            let [lhs, rhs] = args else {
                bail!("binary operation expects exactly 2 arguments")
            };

            let expected_term = ctx.quote_val(&expected);
            let core_arg0 = check(ctx, phase, lhs, expected_term)?;
            let core_arg1 = check(ctx, phase, rhs, expected_term)?;

            let core_args = ctx.alloc_slice([core_arg0, core_arg1]);
            Ok(ctx.alloc(core::Term::new_app(
                ctx.alloc(core::Term::Prim(prim)),
                core_args,
            )))
        }

        // ------------------------------------------------------------------ App { UnOp }
        ast::Term::App {
            func: ast::FunName::UnOp(op),
            args,
        } => {
            let int_ty = match &expected {
                value::Value::Prim(Prim::IntTy(it)) => *it,
                _ => bail!("primitive operation requires an integer type"),
            };

            let prim = match op {
                ast::UnOp::Not => Prim::BitNot(int_ty),
            };

            let [arg] = args else {
                bail!("unary operation expects exactly 1 argument")
            };
            let expected_term = ctx.quote_val(&expected);
            let core_arg = check(ctx, phase, arg, expected_term)?;
            let core_args = std::slice::from_ref(ctx.arena.alloc(core_arg));
            Ok(ctx.alloc(core::Term::new_app(
                ctx.alloc(core::Term::Prim(prim)),
                core_args,
            )))
        }

        // ------------------------------------------------------------------ Quote (check mode)
        ast::Term::Quote(inner) => match &expected {
            value::Value::Lift(obj_ty) => {
                let obj_ty_term = value::quote(ctx.arena, ctx.lvl, obj_ty);
                let core_inner = check(ctx, Phase::Object, inner, obj_ty_term)?;
                Ok(ctx.alloc(core::Term::Quote(core_inner)))
            }
            _ => Err(anyhow!("quote `#(...)` must have a lifted type `[[T]]`")),
        },

        // ------------------------------------------------------------------ Splice (check mode)
        ast::Term::Splice(inner) => {
            ensure!(
                phase == Phase::Object,
                "`$(...)` is only valid in an object-phase context"
            );
            if let value::Value::Prim(Prim::IntTy(IntType {
                width,
                phase: Phase::Object,
            })) = &expected
            {
                let width = *width;
                let expected_term = ctx.quote_val(&expected);
                let lift_ty = ctx.alloc(core::Term::Lift(expected_term));
                if let Ok(core_inner) = check(ctx, Phase::Meta, inner, lift_ty) {
                    return Ok(ctx.alloc(core::Term::Splice(core_inner)));
                }
                let meta_int_ty = ctx.alloc(core::Term::Prim(Prim::IntTy(IntType::meta(width))));
                let core_inner = check(ctx, Phase::Meta, inner, meta_int_ty)?;
                let embedded = ctx.alloc(core::Term::new_app(
                    ctx.alloc(core::Term::Prim(Prim::Embed(width))),
                    ctx.arena.alloc_slice_fill_iter([core_inner]),
                ));
                return Ok(ctx.alloc(core::Term::Splice(embedded)));
            }
            let expected_term = ctx.quote_val(&expected);
            let lift_ty = ctx.alloc(core::Term::Lift(expected_term));
            let core_inner = check(ctx, Phase::Meta, inner, lift_ty)?;
            Ok(ctx.alloc(core::Term::Splice(core_inner)))
        }

        // ------------------------------------------------------------------ Lam (check mode)
        // Check lambda against an expected Pi type.
        ast::Term::Lam { params, body } => {
            ensure!(
                phase == Phase::Meta,
                "lambdas are only valid in meta-phase context"
            );

            let depth_before = ctx.depth();

            // Peel exactly `params.len()` Pi layers from the expected type.
            // This allows nested lambdas: `|a: A| |b: B| body` checks against
            // `fn(_: A) -> fn(_: B) -> R` by covering one Pi layer per lambda.
            let mut pi_params: Vec<(&'_ core::Name, value::Value<'core>)> = Vec::new();
            let mut cur_pi = expected.clone();
            for _ in 0..params.len() {
                match cur_pi {
                    value::Value::Pi(vpi) => {
                        pi_params.push((vpi.name, (*vpi.domain).clone()));
                        let fresh = value::Value::Rigid(Lvl(ctx.depth() + pi_params.len() - 1));
                        cur_pi = value::inst(ctx.arena, &vpi.closure, fresh);
                    }
                    _ => bail!(
                        "lambda has {} parameter(s) but expected type has {}",
                        params.len(),
                        pi_params.len()
                    ),
                }
            }
            let body_ty_val = cur_pi;

            let mut elaborated_params: Vec<(&'core core::Name, &'core core::Term<'core>)> =
                Vec::new();
            for (p, (_, pi_param_ty)) in params.iter().zip(pi_params.into_iter()) {
                let param_name = core::Name::new(ctx.arena.alloc_str(p.name.as_str()));
                let annotated_ty = infer(ctx, Phase::Meta, p.ty)?;
                let annotated_ty_val = ctx.eval(annotated_ty);
                ensure!(
                    types_equal_val(ctx.arena, ctx.lvl, &annotated_ty_val, &pi_param_ty),
                    "lambda parameter type mismatch: annotation gives a different type \
                     than the expected function type"
                );
                elaborated_params.push((param_name, annotated_ty));
                ctx.push_local_val(param_name, pi_param_ty);
            }

            let core_body = check_val(ctx, phase, body, body_ty_val)?;

            for _ in &elaborated_params {
                ctx.pop_local();
            }
            assert_eq!(ctx.depth(), depth_before, "Lam check leaked locals");
            let params_slice = ctx.alloc_slice(elaborated_params);
            Ok(ctx.alloc(core::Term::Lam(Lam {
                params: params_slice,
                body: core_body,
            })))
        }

        // ------------------------------------------------------------------ Match (check mode)
        ast::Term::Match { scrutinee, arms } => {
            let core_scrutinee = infer(ctx, phase, scrutinee)?;
            let scrut_ty_val = ctx.val_type_of(core_scrutinee);

            check_exhaustiveness(&scrut_ty_val, arms)?;

            let scrut_ty_term = ctx.quote_val(&scrut_ty_val);
            let core_arms: &'core [core::Arm<'core>] =
                ctx.arena
                    .alloc_slice_try_fill_iter(arms.iter().map(|arm| -> Result<_> {
                        let core_pat = elaborate_pat(ctx, &arm.pat);
                        if let Some(bname) = core_pat.bound_name() {
                            ctx.push_local(bname, scrut_ty_term);
                        }

                        let arm_result = check_val(ctx, phase, arm.body, expected.clone());

                        if core_pat.bound_name().is_some() {
                            ctx.pop_local();
                        }

                        let core_body = arm_result?;
                        Ok(core::Arm {
                            pat: core_pat,
                            body: core_body,
                        })
                    }))?;

            Ok(ctx.alloc(core::Term::new_match(core_scrutinee, core_arms)))
        }

        // ------------------------------------------------------------------ Block (check mode)
        ast::Term::Block { stmts, expr } => {
            let depth_before = ctx.depth();
            let result = check_block_val(ctx, phase, stmts, expr, expected);
            assert_eq!(ctx.depth(), depth_before, "check_block leaked locals");
            result
        }

        // ------------------------------------------------------------------ fallthrough: infer then unify
        ast::Term::Var(_) | ast::Term::App { .. } | ast::Term::Lift(_) | ast::Term::Pi { .. } => {
            let core_term = infer(ctx, phase, term)?;
            let inferred_val = ctx.val_type_of(core_term);
            ensure!(
                types_equal_val(ctx.arena, ctx.lvl, &inferred_val, &expected),
                "type mismatch"
            );
            Ok(core_term)
        }
    }
}
