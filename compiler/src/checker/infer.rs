use anyhow::{Context as _, Result, anyhow, bail, ensure};

use crate::common::de_bruijn;
use crate::core::{self, IntType, IntWidth, Lam, Phase, Pi, Prim, value};
use crate::parser::ast;

use super::{Ctx, builtin_prim_ty};
use super::ctx::GlobalEntry;

/// Infer the type of a surface term, returning both the elaborated core term
/// and its type as a semantic value.
#[expect(
    clippy::too_many_lines,
    reason = "large match over all surface term variants"
)]
pub fn infer<'names, 'ast, 'core>(
    ctx: &mut Ctx<'names, 'core, '_>,
    phase: Phase,
    term: &'ast ast::Term<'names, 'ast>,
) -> Result<(
    &'core core::Term<'names, 'core>,
    value::Value<'names, 'core>,
)> {
    match term {
        // ------------------------------------------------------------------ Var
        ast::Term::Var(name) => {
            // Built-in type names
            if let Some(term) = builtin_prim_ty(name, phase) {
                if let core::Term::Prim(Prim::U(u_phase)) = term {
                    ensure!(
                        *u_phase == phase,
                        "`{name}` is a {u_phase}-phase type, \
                         not valid in a {phase}-phase context"
                    );
                }
                // Built-in types inhabit the appropriate universe.
                let ty = match term {
                    core::Term::Prim(Prim::IntTy(it)) => value::Value::Prim(Prim::U(it.phase)),
                    core::Term::Prim(Prim::U(_)) => value::Value::Prim(Prim::U(Phase::Meta)),
                    _ => unreachable!("builtin_prim_ty only returns Prim"),
                };
                return Ok((term, ty));
            }
            // Locals
            if let Some((ix, ty)) = ctx.lookup_local(name) {
                let ty = ty.clone();
                return Ok((ctx.alloc(core::Term::Var(ix)), ty));
            }
            // Globals — bare reference without call
            match ctx.globals.get(name) {
                Some(GlobalEntry::Meta(ty_term)) => {
                    let ty = value::eval(ctx.arena, &value::Env::new(), ty_term);
                    return Ok((ctx.alloc(core::Term::Global(name)), ty));
                }
                Some(GlobalEntry::CodeFn { .. }) => {
                    return Err(anyhow!(
                        "`{name}` is a code function and must be called, not referenced directly"
                    ));
                }
                None => {}
            }
            Err(anyhow!("unbound variable `{name}`"))
        }

        // ------------------------------------------------------------------ Lit
        ast::Term::Lit(_) => Err(anyhow!(
            "cannot infer type of a literal; add a type annotation"
        )),

        // ------------------------------------------------------------------ App { Global or local }
        ast::Term::App {
            func: ast::FunName::Term(func_term),
            args,
        } => {
            // Special case: direct call to a code function.
            // Code functions have no first-class type term, so we handle them before
            // calling infer on func_term (which would otherwise error on a bare code ref).
            if let ast::Term::Var(fname) = func_term {
                if let Some(GlobalEntry::CodeFn { params, ret_ty }) = ctx.globals.get(fname) {
                    ensure!(
                        phase == Phase::Object,
                        "code function `{fname}` can only be called in object-phase context"
                    );
                    ensure!(
                        args.len() == params.len(),
                        "wrong number of arguments: `{fname}` expects {}, got {}",
                        params.len(),
                        args.len()
                    );
                    let mut core_args: Vec<&'core core::Term<'names, 'core>> =
                        Vec::with_capacity(args.len());
                    for (i, (arg, (_, param_ty))) in args.iter().zip(params.iter()).enumerate() {
                        let param_val = ctx.eval(param_ty);
                        let core_arg = check_val(ctx, phase, arg, param_val)
                            .with_context(|| format!("in argument {i} of call to `{fname}`"))?;
                        core_args.push(core_arg);
                    }
                    let callee = ctx.alloc(core::Term::Global(fname));
                    let args_slice = ctx.alloc_slice(core_args);
                    let result_ty = ctx.eval(ret_ty);
                    return Ok((ctx.alloc(core::Term::new_app(callee, args_slice)), result_ty));
                }
            }

            // General case: meta function or higher-order call via Pi type.
            let (callee, callee_ty) = infer(ctx, phase, func_term)?;
            let vpi = match &callee_ty {
                value::Value::Pi(vpi) => vpi.clone(),
                _ => bail!("callee is not a function"),
            };
            ensure!(
                args.len() == vpi.params.len(),
                "wrong number of arguments: callee expects {}, got {}",
                vpi.params.len(),
                args.len()
            );

            // Check each arg against its domain (evaluated with prior arg values).
            let mut arg_vals: Vec<value::Value<'names, 'core>> = Vec::with_capacity(args.len());
            let mut core_args: Vec<&'core core::Term<'names, 'core>> =
                Vec::with_capacity(args.len());
            for (i, (arg, (_, domain_cl))) in args.iter().zip(vpi.params.iter()).enumerate() {
                let domain_val = value::inst_n(ctx.arena, domain_cl, &arg_vals);
                let core_arg = check_val(ctx, phase, arg, domain_val)
                    .with_context(|| format!("in argument {i} of function call"))?;
                let arg_val = ctx.eval(core_arg);
                arg_vals.push(arg_val);
                core_args.push(core_arg);
            }

            // Evaluate return type with all arg values.
            let result_ty = value::inst_n(ctx.arena, &vpi.ret_closure, &arg_vals);

            let args_slice = ctx.alloc_slice(core_args);
            Ok((
                ctx.alloc(core::Term::new_app(callee, args_slice)),
                result_ty,
            ))
        }

        // ------------------------------------------------------------------ App { Prim (comparison) }
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

            let (core_arg0, operand_ty_val) = infer(ctx, phase, lhs)?;
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
            let result_ty = value::Value::Prim(Prim::IntTy(IntType {
                width: IntWidth::U1,
                phase: op_int_ty.phase,
            }));
            Ok((
                ctx.alloc(core::Term::new_app(
                    ctx.alloc(prim.into()),
                    ctx.alloc_slice([core_arg0, core_arg1]),
                )),
                result_ty,
            ))
        }
        ast::Term::App {
            func: ast::FunName::BinOp(_) | ast::FunName::UnOp(_),
            ..
        } => Err(anyhow!(
            "cannot infer type of a primitive operation; add a type annotation"
        )),

        // ------------------------------------------------------------------ Pi
        ast::Term::Pi { params, ret_ty } => {
            ensure!(
                phase.is_meta(),
                "function types are only valid in meta-phase context"
            );
            let depth_before = ctx.depth();

            let mut elaborated_params: Vec<(&'names core::Name, &'core core::Term<'names, 'core>)> =
                Vec::new();
            for p in *params {
                let param_ty = check_universe(ctx, Phase::Meta, p.ty)?;
                elaborated_params.push((p.name, param_ty));
                ctx.push_local(p.name, param_ty);
            }

            let core_ret_ty = check_universe(ctx, Phase::Meta, ret_ty)?;

            for _ in &elaborated_params {
                ctx.pop_local();
            }
            assert_eq!(ctx.depth(), depth_before, "Pi elaboration leaked locals");
            let params_slice = ctx.alloc_slice(elaborated_params);
            Ok((
                ctx.alloc(core::Term::Pi(Pi {
                    params: params_slice,
                    body_ty: core_ret_ty,
                })),
                value::Value::Prim(Prim::U(Phase::Meta)),
            ))
        }

        // ------------------------------------------------------------------ Lam (infer mode)
        ast::Term::Lam {
            params,
            ret_ty,
            body,
        } => {
            ensure!(
                phase.is_meta(),
                "lambdas are only valid in meta-phase context"
            );

            let depth_before = ctx.depth();
            let mut elaborated_params: Vec<(&'names core::Name, &'core core::Term<'names, 'core>)> =
                Vec::new();

            for p in *params {
                let (param_ty, _) = infer(ctx, Phase::Meta, p.ty)?;
                elaborated_params.push((p.name, param_ty));
                ctx.push_local(p.name, param_ty);
            }

            // Elaborate the body inside the param scope. Delay the `?` so that
            // params are always popped even when the body fails to elaborate.
            let body_result = if let Some(ret_ty_ann) = ret_ty {
                // Explicit return type annotation: check body against it.
                infer(ctx, Phase::Meta, ret_ty_ann).and_then(|(ret_ty_core, _)| {
                    let ret_ty_val = ctx.eval(ret_ty_core);
                    let core_body = check_val(ctx, phase, body, ret_ty_val.clone())?;
                    Ok((core_body, ret_ty_val))
                })
            } else {
                infer(ctx, phase, body)
            };

            // Quote the body type before popping (while params are still in scope).
            let body_ty_term = body_result
                .as_ref()
                .ok()
                .map(|(_, body_ty)| value::quote(ctx.arena, ctx.depth(), body_ty));

            for _ in &elaborated_params {
                ctx.pop_local();
            }
            assert_eq!(ctx.depth(), depth_before, "Lam elaboration leaked locals");

            let (core_body, _body_ty) = body_result?;
            // Safe: body_ty_term is Some whenever body_result is Ok (computed before the ? above).
            let body_ty_term = body_ty_term.expect("body_ty_term is Some when body_result is Ok");
            let params_slice = ctx.alloc_slice(elaborated_params);

            // Build the Pi value for the inferred type.
            let pi_term = ctx.alloc(core::Term::Pi(Pi {
                params: params_slice,
                body_ty: body_ty_term,
            }));
            let pi_val = ctx.eval(pi_term);

            Ok((
                ctx.alloc(core::Term::Lam(Lam {
                    params: params_slice,
                    body: core_body,
                })),
                pi_val,
            ))
        }

        // ------------------------------------------------------------------ Lift
        ast::Term::Lift(inner) => {
            ensure!(
                phase.is_meta(),
                "`[[...]]` is only valid in a meta-phase context"
            );
            let core_inner = check_universe(ctx, Phase::Object, inner)?;
            Ok((
                ctx.alloc(core::Term::Lift(core_inner)),
                value::Value::Prim(Prim::U(Phase::Meta)),
            ))
        }

        // ------------------------------------------------------------------ Quote
        ast::Term::Quote(inner) => {
            ensure!(
                phase.is_meta(),
                "`#(...)` is only valid in a meta-phase context"
            );
            let (core_inner, inner_ty) = infer(ctx, Phase::Object, inner)?;
            Ok((
                ctx.alloc(core::Term::Quote(core_inner)),
                value::Value::Lift(ctx.arena.alloc(inner_ty)),
            ))
        }

        // ------------------------------------------------------------------ Splice
        ast::Term::Splice(inner) => {
            ensure!(
                phase.is_object(),
                "`$(...)` is only valid in an object-phase context"
            );
            let (core_inner, inner_ty_val) = infer(ctx, Phase::Meta, inner)?;
            match inner_ty_val {
                value::Value::Lift(obj_ty) => {
                    Ok((ctx.alloc(core::Term::Splice(core_inner)), (*obj_ty).clone()))
                }
                value::Value::Prim(Prim::IntTy(IntType {
                    width,
                    phase: Phase::Meta,
                })) => {
                    let embedded = ctx.alloc(core::Term::new_app(
                        ctx.alloc(core::Term::Prim(Prim::Embed(width))),
                        ctx.alloc_slice([core_inner]),
                    ));
                    let obj_ty = value::Value::Prim(Prim::IntTy(IntType {
                        width,
                        phase: Phase::Object,
                    }));
                    Ok((ctx.alloc(core::Term::Splice(embedded)), obj_ty))
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

/// Check exhaustiveness of `arms` given the scrutinee type `scrut_ty`.
fn check_exhaustiveness(
    scrut_ty: &value::Value<'_, '_>,
    arms: &[ast::MatchArm<'_, '_>],
) -> Result<()> {
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
fn elaborate_pat<'names>(pat: &ast::Pat<'names>) -> core::Pat<'names> {
    match pat {
        ast::Pat::Lit(n) => core::Pat::Lit(*n),
        ast::Pat::Name(name) => match name.as_str() {
            "_" => core::Pat::Wildcard,
            _ => core::Pat::Bind(name),
        },
    }
}

/// Elaborate a single `let` binding.
fn elaborate_let<'names, 'ast, 'core, T, F, G, W>(
    ctx: &mut Ctx<'names, 'core, '_>,
    phase: Phase,
    stmt: &'ast ast::Let<'names, 'ast>,
    cont: F,
    body_of: G,
    wrap: W,
) -> Result<T>
where
    F: FnOnce(&mut Ctx<'names, 'core, '_>) -> Result<T>,
    G: FnOnce(&T) -> &'core core::Term<'names, 'core>,
    W: FnOnce(&'core core::Term<'names, 'core>, T) -> T,
{
    let (core_expr, bind_ty_val) = if let Some(ann) = stmt.ty {
        let (ty, _) = infer(ctx, phase, ann)?;
        let ty_val = ctx.eval(ty);
        let core_e = check_val(ctx, phase, stmt.expr, ty_val.clone())
            .with_context(|| format!("in let binding `{}`", stmt.name.as_str()))?;
        (core_e, ty_val)
    } else {
        let (core_e, bind_ty) = infer(ctx, phase, stmt.expr)
            .with_context(|| format!("in let binding `{}`", stmt.name.as_str()))?;
        (core_e, bind_ty)
    };

    let bind_ty_term = ctx.quote_val(&bind_ty_val);
    // Evaluate the bound expression so dependent references to this binding work correctly.
    let expr_val = ctx.eval(core_expr);
    let bind_name = stmt.name;
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
fn infer_block<'names, 'ast, 'core>(
    ctx: &mut Ctx<'names, 'core, '_>,
    phase: Phase,
    stmts: &'ast [ast::Let<'names, 'ast>],
    expr: &'ast ast::Term<'names, 'ast>,
) -> Result<(
    &'core core::Term<'names, 'core>,
    value::Value<'names, 'core>,
)> {
    match stmts {
        [] => infer(ctx, phase, expr),
        [first, rest @ ..] => elaborate_let(
            ctx,
            phase,
            first,
            |ctx| infer_block(ctx, phase, rest, expr),
            |(body, _ty)| body,
            |let_term, (_body, ty)| (let_term, ty),
        ),
    }
}

/// Elaborate a sequence of `let` bindings followed by a trailing expression (check mode).
fn check_block_val<'names, 'ast, 'core>(
    ctx: &mut Ctx<'names, 'core, '_>,
    phase: Phase,
    stmts: &'ast [ast::Let<'names, 'ast>],
    expr: &'ast ast::Term<'names, 'ast>,
    expected: value::Value<'names, 'core>,
    expected_term: Option<&'core core::Term<'names, 'core>>,
) -> Result<&'core core::Term<'names, 'core>> {
    match stmts {
        [] => check_val_impl(ctx, phase, expr, expected, expected_term),
        [first, rest @ ..] => elaborate_let(
            ctx,
            phase,
            first,
            |ctx| check_block_val(ctx, phase, rest, expr, expected.clone(), expected_term),
            |body| body,
            |let_term, _body| let_term,
        ),
    }
}

/// Check that `term` elaborates as a type in the given phase's universe.
///
/// Equivalent to `check(ctx, phase, term, Type)` for meta phase or
/// `check(ctx, phase, term, VmType)` for object phase.
fn check_universe<'names, 'ast, 'core>(
    ctx: &mut Ctx<'names, 'core, '_>,
    phase: Phase,
    term: &'ast ast::Term<'names, 'ast>,
) -> Result<&'core core::Term<'names, 'core>> {
    let universe: &core::Term = match phase {
        Phase::Meta => &core::Term::TYPE,
        Phase::Object => &core::Term::VM_TYPE,
    };
    check(ctx, phase, term, universe)
}

/// Check `term` against `expected` (as a term reference), returning the elaborated core term.
///
/// This is a convenience wrapper for callers that have an expected type as a `&Term`.
/// It also threads the expected term through for dependent-type arm refinement.
pub fn check<'names, 'ast, 'core>(
    ctx: &mut Ctx<'names, 'core, '_>,
    phase: Phase,
    term: &'ast ast::Term<'names, 'ast>,
    expected: &'core core::Term<'names, 'core>,
) -> Result<&'core core::Term<'names, 'core>> {
    let expected_val = ctx.eval(expected);
    check_val_impl(ctx, phase, term, expected_val, Some(expected))
}

/// Check `term` against `expected` (as a semantic Value), returning the elaborated core term.
pub fn check_val<'names, 'ast, 'core>(
    ctx: &mut Ctx<'names, 'core, '_>,
    phase: Phase,
    term: &'ast ast::Term<'names, 'ast>,
    expected: value::Value<'names, 'core>,
) -> Result<&'core core::Term<'names, 'core>> {
    check_val_impl(ctx, phase, term, expected, None)
}

/// Internal implementation — `expected_term` carries the original core term for the expected
/// type, enabling dependent-type arm refinement (re-evaluating under a modified env).
#[expect(
    clippy::too_many_lines,
    reason = "large match over all surface term variants"
)]
fn check_val_impl<'names, 'ast, 'core>(
    ctx: &mut Ctx<'names, 'core, '_>,
    phase: Phase,
    term: &'ast ast::Term<'names, 'ast>,
    expected: value::Value<'names, 'core>,
    expected_term: Option<&'core core::Term<'names, 'core>>,
) -> Result<&'core core::Term<'names, 'core>> {
    match term {
        // ------------------------------------------------------------------ Lit
        ast::Term::Lit(n) => match &expected {
            value::Value::Prim(Prim::IntTy(it)) => {
                ensure!(
                    it.phase == phase,
                    "literal checked at {phase} phase but expected type is {}-phase",
                    it.phase
                );
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
            use ast::BinOp;
            let int_ty = match &expected {
                value::Value::Prim(Prim::IntTy(it)) => *it,
                _ => bail!("primitive operation requires an integer type"),
            };

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

            Ok(ctx.alloc(core::Term::new_app(
                ctx.alloc(prim.into()),
                ctx.alloc_slice([core_arg0, core_arg1]),
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
            Ok(ctx.alloc(core::Term::new_app(ctx.alloc(prim.into()), core_args)))
        }

        // ------------------------------------------------------------------ Quote (check mode)
        ast::Term::Quote(inner) => match &expected {
            value::Value::Lift(obj_ty) => {
                let obj_ty_term = value::quote(ctx.arena, ctx.depth(), obj_ty);
                let core_inner = check(ctx, Phase::Object, inner, obj_ty_term)?;
                Ok(ctx.alloc(core::Term::Quote(core_inner)))
            }
            _ => Err(anyhow!("quote `#(...)` must have a lifted type `[[T]]`")),
        },

        // ------------------------------------------------------------------ Splice (check mode)
        ast::Term::Splice(inner) => {
            ensure!(
                phase.is_object(),
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
        ast::Term::Lam {
            params,
            ret_ty,
            body,
        } => {
            ensure!(
                phase.is_meta(),
                "lambdas are only valid in meta-phase context"
            );

            let depth_before = ctx.depth();

            // Expected type must be a Pi with matching arity.
            let vpi = match &expected {
                value::Value::Pi(vpi) => vpi.clone(),
                _ => bail!("lambda requires a function type"),
            };
            ensure!(
                params.len() == vpi.params.len(),
                "lambda has {} parameter(s) but expected type has {}",
                params.len(),
                vpi.params.len()
            );

            let mut elaborated_params: Vec<(&'names core::Name, &'core core::Term<'names, 'core>)> =
                Vec::new();
            let mut arg_vals: Vec<value::Value<'names, 'core>> = Vec::new();

            // Elaborate params and body inside a closure so that `?`/`ensure!` return
            // from the closure rather than the outer function, allowing locals to be
            // popped unconditionally after the closure call.
            let core_body_result: Result<_> = (|| {
                for (p, (_, domain_cl)) in params.iter().zip(vpi.params.iter()) {
                    let param_name = p.name;
                    let (annotated_ty, _) = infer(ctx, Phase::Meta, p.ty)?;
                    let annotated_ty_val = ctx.eval(annotated_ty);
                    let expected_domain = value::inst_n(ctx.arena, domain_cl, &arg_vals);
                    ensure!(
                        value::val_eq(ctx.depth(), &annotated_ty_val, &expected_domain),
                        "lambda parameter type mismatch: annotation gives a different type \
                         than the expected function type"
                    );
                    elaborated_params.push((param_name, annotated_ty));
                    let lvl = ctx.depth().as_lvl();
                    ctx.push_local_val(param_name, expected_domain);
                    arg_vals.push(value::Value::Rigid(lvl));
                }

                let body_ty_val = value::inst_n(ctx.arena, &vpi.ret_closure, &arg_vals);

                // If an explicit return type annotation is present, verify it matches.
                if let Some(ret_ty_ann) = ret_ty {
                    let (ret_ty_core, _) = infer(ctx, Phase::Meta, ret_ty_ann)?;
                    let ret_ty_ann_val = ctx.eval(ret_ty_core);
                    ensure!(
                        value::val_eq(ctx.depth(), &ret_ty_ann_val, &body_ty_val),
                        "lambda return type annotation does not match the expected type"
                    );
                }

                check_val(ctx, phase, body, body_ty_val)
            })();

            // Pop however many params were successfully pushed, regardless of outcome.
            for _ in &elaborated_params {
                ctx.pop_local();
            }
            assert_eq!(ctx.depth(), depth_before, "Lam check leaked locals");
            let params_slice = ctx.alloc_slice(elaborated_params);
            let core_body = core_body_result?;
            Ok(ctx.alloc(core::Term::Lam(Lam {
                params: params_slice,
                body: core_body,
            })))
        }

        // ------------------------------------------------------------------ Match (check mode)
        ast::Term::Match { scrutinee, arms } => {
            let (core_scrutinee, scrut_ty_val) = infer(ctx, phase, scrutinee)?;

            check_exhaustiveness(&scrut_ty_val, arms)?;

            let scrut_ty_term = ctx.quote_val(&scrut_ty_val);

            // For dependent return types: if the scrutinee is a rigid variable and we
            // have the expected type as a core term, refine per-arm by re-evaluating
            // that term with the arm's literal substituted for the scrutinee variable.
            let scrut_val = ctx.eval(core_scrutinee);
            let scrut_refine: Option<(de_bruijn::Lvl, IntType)> = match (&scrut_val, &scrut_ty_val)
            {
                (value::Value::Rigid(lvl), value::Value::Prim(Prim::IntTy(it))) => {
                    Some((*lvl, *it))
                }
                _ => None,
            };

            let core_arms: &'core [core::Arm<'names, 'core>] = ctx
                .arena
                .alloc_slice_try_fill_iter(arms.iter().map(|arm| -> Result<_> {
                    let core_pat = elaborate_pat(&arm.pat);

                    let arm_expected = match (&scrut_refine, &core_pat, expected_term) {
                        (Some((lvl, int_ty)), core::Pat::Lit(n), Some(ety)) => {
                            let mut env = ctx.value_env();
                            env[*lvl] = value::Value::Lit(*n, *int_ty);
                            value::eval(ctx.arena, &env, ety)
                        }
                        _ => expected.clone(),
                    };

                    if let Some(bname) = core_pat.bound_name() {
                        ctx.push_local(bname, scrut_ty_term);
                    }

                    let arm_result = check_val(ctx, phase, arm.body, arm_expected);

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
            let result = check_block_val(ctx, phase, stmts, expr, expected, expected_term);
            assert_eq!(ctx.depth(), depth_before, "check_block leaked locals");
            result
        }

        // ------------------------------------------------------------------ fallthrough: infer then unify
        ast::Term::Var(_) | ast::Term::App { .. } | ast::Term::Lift(_) | ast::Term::Pi { .. } => {
            let (core_term, inferred_val) = infer(ctx, phase, term)?;
            ensure!(
                value::val_eq(ctx.depth(), &inferred_val, &expected),
                "type mismatch"
            );
            Ok(core_term)
        }
    }
}
