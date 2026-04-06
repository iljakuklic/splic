use std::collections::HashMap;

use anyhow::{Context as _, Result, ensure};

use crate::core::{self, Pi};
use crate::parser::ast;

use super::Ctx;
use super::infer;

/// Elaborate one function's signature into a `Pi` (the globals table entry).
fn elaborate_sig<'names, 'ast, 'core>(
    arena: &'core bumpalo::Bump,
    func: &ast::Function<'names, 'ast>,
) -> Result<&'core core::Pi<'names, 'core>> {
    let empty_globals: HashMap<&'names core::Name, &'core core::Pi<'names, 'core>> = HashMap::new();
    let mut ctx = Ctx::new(arena, &empty_globals);

    let params: &'core [(&'names core::Name, &'core core::Term<'names, 'core>)] = arena
        .alloc_slice_try_fill_iter(func.params.iter().map(|p| -> Result<_> {
            let (param_ty, _) = infer::infer(&mut ctx, func.phase, p.ty)?;
            ctx.push_local(p.name, param_ty);
            Ok((p.name, param_ty))
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
pub fn collect_signatures<'names, 'ast, 'core>(
    arena: &'core bumpalo::Bump,
    program: &ast::Program<'names, 'ast>,
) -> Result<HashMap<&'names core::Name, &'core core::Pi<'names, 'core>>> {
    let mut globals: HashMap<&'names core::Name, &'core core::Pi<'names, 'core>> = HashMap::new();

    for func in program.functions {
        let name = func.name;

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
fn elaborate_bodies<'names, 'ast, 'core>(
    arena: &'core bumpalo::Bump,
    program: &ast::Program<'names, 'ast>,
    globals: &HashMap<&'names core::Name, &'core core::Pi<'names, 'core>>,
) -> Result<core::Program<'names, 'core>> {
    let functions: &'core [core::Function<'names, 'core>] =
        arena.alloc_slice_try_fill_iter(program.functions.iter().map(|func| -> Result<_> {
            let name = func.name;
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
pub fn elaborate_program<'names, 'core>(
    arena: &'core bumpalo::Bump,
    program: &ast::Program<'names, '_>,
) -> Result<core::Program<'names, 'core>> {
    let globals = collect_signatures(arena, program)?;
    elaborate_bodies(arena, program, &globals)
}
