use std::collections::HashMap;

use anyhow::{Context as _, Result, bail, ensure};

use crate::core::{self, Phase};
use crate::parser::ast;

use super::Ctx;
use super::ctx::GlobalEntry;
use super::infer;

/// Elaborate one definition's signature into a `GlobalEntry`.
///
/// Meta-phase definitions with a Pi type annotation produce `GlobalEntry::Meta(Pi)`.
/// Meta-phase constant definitions produce `GlobalEntry::Meta(ty)`.
/// Object-phase definitions with a Pi annotation produce `GlobalEntry::CodeFn { params, ret_ty }`.
/// Object-phase non-function definitions are rejected (not yet supported).
fn elaborate_sig<'names, 'ast, 'core>(
    arena: &'core bumpalo::Bump,
    def: &ast::GlobalDef<'names, 'ast>,
) -> Result<GlobalEntry<'names, 'core>> {
    let empty_globals: HashMap<&'names core::Name, GlobalEntry<'names, 'core>> = HashMap::new();
    let mut ctx = Ctx::new(arena, &empty_globals);

    match (def.phase, def.ty) {
        (phase, ast::Term::Pi { params, ret_ty }) => {
            let universe = arena.alloc(core::Term::universe(phase));
            let core_params =
                arena.alloc_slice_try_fill_iter(params.iter().map(|p| -> Result<_> {
                    let ty = infer::check(&mut ctx, phase, p.ty, universe)?;
                    ctx.push_local(p.name, ty);
                    Ok((p.name, ty))
                }))?;
            let core_ret_ty = infer::check(&mut ctx, phase, ret_ty, universe)?;
            if phase == Phase::Meta {
                let pi = arena.alloc(core::Term::Pi(core::Pi {
                    params: core_params,
                    body_ty: core_ret_ty,
                }));
                Ok(GlobalEntry::Meta(pi))
            } else {
                Ok(GlobalEntry::CodeFn {
                    params: core_params,
                    ret_ty: core_ret_ty,
                })
            }
        }
        (Phase::Meta, ty) => {
            let universe = arena.alloc(core::Term::universe(Phase::Meta));
            let core_ty = infer::check(&mut ctx, Phase::Meta, ty, universe)?;
            Ok(GlobalEntry::Meta(core_ty))
        }
        (Phase::Object, _) => {
            bail!("object-level constants are not supported")
        }
    }
}

/// Pass 1: collect all top-level definition signatures into a globals table.
pub fn collect_signatures<'names, 'ast, 'core>(
    arena: &'core bumpalo::Bump,
    program: &ast::Program<'names, 'ast>,
) -> Result<HashMap<&'names core::Name, GlobalEntry<'names, 'core>>> {
    let mut globals: HashMap<&'names core::Name, GlobalEntry<'names, 'core>> = HashMap::new();

    for def in program.defs {
        let name = def.name;

        ensure!(
            !globals.contains_key(&name),
            "duplicate function name `{name}`"
        );

        let entry = elaborate_sig(arena, def).with_context(|| format!("in `{name}`"))?;

        globals.insert(name, entry);
    }

    Ok(globals)
}

/// Pass 2: elaborate all definition bodies with the full globals table available.
fn elaborate_bodies<'names, 'ast, 'core>(
    arena: &'core bumpalo::Bump,
    program: &ast::Program<'names, 'ast>,
    globals: &HashMap<&'names core::Name, GlobalEntry<'names, 'core>>,
) -> Result<core::Program<'names, 'core>> {
    let defs: &'core [core::GlobalDef<'names, 'core>] =
        arena.alloc_slice_try_fill_iter(program.defs.iter().map(|def| -> Result<_> {
            let name = def.name;
            let mut ctx = Ctx::new(arena, globals);

            let global = match globals.get(&name).expect("signature missing from pass 1") {
                GlobalEntry::Meta(ty) => {
                    // For function definitions (Pi type + Lam body), manually push the Pi
                    // params into scope and check the Lam's inner body against pi.body_ty.
                    // This preserves `expected_term` through to match arm refinement —
                    // necessary for dependent return types.
                    let body = match (*ty, def.body) {
                        (core::Term::Pi(pi), ast::Term::Lam { body: lam_body, .. }) => {
                            for (param_name, param_ty) in pi.params {
                                ctx.push_local(param_name, param_ty);
                            }
                            let core_body =
                                infer::check(&mut ctx, Phase::Meta, lam_body, pi.body_ty)
                                    .with_context(|| format!("in `{name}`"))?;
                            for _ in pi.params {
                                ctx.pop_local();
                            }
                            arena.alloc(core::Term::Lam(core::Lam {
                                params: pi.params,
                                body: core_body,
                            }))
                        }
                        _ => infer::check(&mut ctx, Phase::Meta, def.body, ty)
                            .with_context(|| format!("in `{name}`"))?,
                    };
                    core::Global::Meta(core::GlobalMeta { ty, body })
                }
                GlobalEntry::CodeFn { params, ret_ty } => {
                    let ast::Term::Lam { body: lam_body, .. } = def.body else {
                        bail!("in `{name}`: code function body must be a lambda");
                    };
                    for (param_name, param_ty) in *params {
                        ctx.push_local(param_name, param_ty);
                    }
                    let core_body = infer::check(&mut ctx, Phase::Object, lam_body, ret_ty)
                        .with_context(|| format!("in `{name}`"))?;
                    for _ in *params {
                        ctx.pop_local();
                    }
                    core::Global::CodeFn(core::CodeFn {
                        params,
                        ret_ty,
                        body: core_body,
                    })
                }
            };

            Ok(core::GlobalDef { name, global })
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
