use std::collections::HashMap;

use anyhow::{Context as _, Result, ensure};

use crate::core::{self, Pi};
use crate::parser::ast;

use super::Ctx;
use super::infer;

/// Elaborate one function's signature into a `Pi` (the globals table entry).
fn elaborate_sig<'src, 'core>(
    arena: &'core bumpalo::Bump,
    func: &ast::Function<'src>,
) -> Result<&'core core::Pi<'core>> {
    let empty_globals = HashMap::new();
    let mut ctx = Ctx::new(arena, &empty_globals);

    let params: &'core [(&'core core::Name, &'core core::Term<'core>)] = arena
        .alloc_slice_try_fill_iter(func.params.iter().map(|p| -> Result<_> {
            let param_name = core::Name::new(arena.alloc_str(p.name.as_str()));
            let (param_ty, _) = infer::infer(&mut ctx, func.phase, p.ty)?;
            ctx.push_local(param_name, param_ty);
            Ok((param_name, param_ty))
        }))?;

    let body_ty = infer::check(
        &mut ctx,
        func.phase,
        func.ret_ty,
        core::Term::universe(func.phase),
    )?;

    Ok(arena.alloc(Pi {
        params,
        body_ty,
        phase: func.phase,
    }))
}

/// Pass 1: collect all top-level function signatures into a globals table.
pub fn collect_signatures<'src, 'core>(
    arena: &'core bumpalo::Bump,
    program: &ast::Program<'src>,
) -> Result<HashMap<&'core core::Name, &'core core::Pi<'core>>> {
    let mut globals: HashMap<&'core core::Name, &'core core::Pi<'core>> = HashMap::new();

    for func in program.functions {
        let name = core::Name::new(arena.alloc_str(func.name.as_str()));

        ensure!(
            !globals.contains_key(&name),
            "duplicate function name `{name}`"
        );

        let ty = elaborate_sig(arena, func).with_context(|| format!("in function `{name}`"))?;

        globals.insert(name, ty);
    }

    Ok(globals)
}

/// Pass 2: elaborate all function bodies with the full globals table available.
fn elaborate_bodies<'src, 'core>(
    arena: &'core bumpalo::Bump,
    program: &ast::Program<'src>,
    globals: &HashMap<&'core core::Name, &'core core::Pi<'core>>,
) -> Result<core::Program<'core>> {
    let functions: &'core [core::Function<'core>] =
        arena.alloc_slice_try_fill_iter(program.functions.iter().map(|func| -> Result<_> {
            let name = core::Name::new(arena.alloc_str(func.name.as_str()));
            let pi = *globals.get(&name).expect("signature missing from pass 1");

            // Build a fresh context borrowing the stack-owned globals map.
            let mut ctx = Ctx::new(arena, globals);

            // Push parameters as locals so the body can reference them.
            for (pname, pty) in pi.params {
                ctx.push_local(pname, pty);
            }

            // Elaborate the body, checking it against the declared return type.
            // Using `check` (rather than pre-evaluating) threads the core term through
            // so the checker can refine dependent return types per match arm.
            let body = infer::check(&mut ctx, pi.phase, func.body, pi.body_ty)
                .with_context(|| format!("in function `{name}`"))?;

            Ok(core::Function { name, ty: pi, body })
        }))?;

    Ok(core::Program { functions })
}

/// Elaborate the entire program in two passes
pub fn elaborate_program<'core>(
    arena: &'core bumpalo::Bump,
    program: &ast::Program<'_>,
) -> Result<core::Program<'core>> {
    let globals = collect_signatures(arena, program)?;
    elaborate_bodies(arena, program, &globals)
}
