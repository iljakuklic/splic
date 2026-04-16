use std::collections::HashMap;

use anyhow::{Context as _, Result, ensure};

use crate::core;
use crate::parser::ast;

use super::Ctx;
use super::infer;

/// Elaborate one definition's type annotation into a core type term.
///
/// For function definitions (Pi type), each parameter and the return type are
/// checked individually at `def.phase` and assembled into a `core::Pi`.
/// For constant definitions, the type annotation is checked directly.
fn elaborate_sig<'names, 'ast, 'core>(
    arena: &'core bumpalo::Bump,
    def: &ast::GlobalDef<'names, 'ast>,
) -> Result<&'core core::Term<'names, 'core>> {
    let empty_globals: HashMap<&'names core::Name, &'core core::Term<'names, 'core>> =
        HashMap::new();
    let mut ctx = Ctx::new(arena, &empty_globals);
    let universe = arena.alloc(core::Term::universe(def.phase));

    if let ast::Term::Pi { params, ret_ty } = def.ty {
        // Pi types are meta-only in the checker, but object-phase functions also
        // have Pi types. Elaborate param and return types individually so the
        // resulting `core::Pi` carries the correct `def.phase`.
        let core_params = arena.alloc_slice_try_fill_iter(params.iter().map(|p| -> Result<_> {
            let ty = infer::check(&mut ctx, def.phase, p.ty, universe)?;
            ctx.push_local(p.name, ty);
            Ok((p.name, ty))
        }))?;
        let core_ret_ty = infer::check(&mut ctx, def.phase, ret_ty, universe)?;
        Ok(arena.alloc(core::Term::Pi(core::Pi {
            params: core_params,
            body_ty: core_ret_ty,
            phase: def.phase,
        })))
    } else {
        infer::check(&mut ctx, def.phase, def.ty, universe)
    }
}

/// Pass 1: collect all top-level definition signatures into a globals table.
pub fn collect_signatures<'names, 'ast, 'core>(
    arena: &'core bumpalo::Bump,
    program: &ast::Program<'names, 'ast>,
) -> Result<HashMap<&'names core::Name, &'core core::Term<'names, 'core>>> {
    let mut globals: HashMap<&'names core::Name, &'core core::Term<'names, 'core>> =
        HashMap::new();

    for def in program.defs {
        let name = def.name;

        ensure!(
            !globals.contains_key(&name),
            "duplicate function name `{name}`"
        );

        let ty = elaborate_sig(arena, def).with_context(|| format!("in function `{name}`"))?;

        globals.insert(name, ty);
    }

    Ok(globals)
}

/// Pass 2: elaborate all definition bodies with the full globals table available.
fn elaborate_bodies<'names, 'ast, 'core>(
    arena: &'core bumpalo::Bump,
    program: &ast::Program<'names, 'ast>,
    globals: &HashMap<&'names core::Name, &'core core::Term<'names, 'core>>,
) -> Result<core::Program<'names, 'core>> {
    let defs: &'core [core::GlobalDef<'names, 'core>] =
        arena.alloc_slice_try_fill_iter(program.defs.iter().map(|def| -> Result<_> {
            let name = def.name;
            let ty = *globals.get(&name).expect("signature missing from pass 1");

            let mut ctx = Ctx::new(arena, globals);

            // For function definitions (Pi type + Lam body), manually push the Pi
            // params into scope and check the Lam's inner body against pi.body_ty.
            // This matches the old behavior of elaborating params and body directly,
            // which preserves `expected_term` through to match arm refinement —
            // necessary for dependent return types. Going through the Lam checker
            // instead would lose `expected_term` (it calls `check_val`, not `check`).
            let body = match (ty, def.body) {
                (core::Term::Pi(pi), ast::Term::Lam { body: lam_body, .. }) => {
                    for (param_name, param_ty) in pi.params {
                        ctx.push_local(param_name, param_ty);
                    }
                    let core_body = infer::check(&mut ctx, def.phase, lam_body, pi.body_ty)
                        .with_context(|| format!("in `{name}`"))?;
                    for _ in pi.params {
                        ctx.pop_local();
                    }
                    arena.alloc(core::Term::Lam(core::Lam {
                        params: pi.params,
                        body: core_body,
                    }))
                }
                _ => infer::check(&mut ctx, def.phase, def.body, ty)
                    .with_context(|| format!("in `{name}`"))?,
            };

            Ok(core::GlobalDef { name, phase: def.phase, ty, body })
        }))?;

    Ok(core::Program { defs })
}

/// Elaborate the entire program in two passes
pub fn elaborate_program<'names, 'core>(
    arena: &'core bumpalo::Bump,
    program: &ast::Program<'names, '_>,
) -> Result<core::Program<'names, 'core>> {
    let globals = collect_signatures(arena, program)?;
    elaborate_bodies(arena, program, &globals)
}
