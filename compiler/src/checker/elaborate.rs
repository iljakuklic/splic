use std::collections::HashMap;

use anyhow::{Context as _, Result, ensure};

use crate::core::{self, Phase};
use crate::parser::ast;

use super::Ctx;
use super::ctx::GlobalEntry;
use super::infer;

/// Elaborate one definition's signature into a `GlobalEntry`.
fn elaborate_sig<'names, 'ast, 'core>(
    arena: &'core bumpalo::Bump,
    def: &ast::GlobalDef<'names, 'ast>,
) -> Result<GlobalEntry<'names, 'core>> {
    let empty_globals: HashMap<&'names core::Name, GlobalEntry<'names, 'core>> = HashMap::new();
    let mut ctx = Ctx::new(arena, &empty_globals);
    let universe = core::Term::universe(def.phase);

    let core_params = def
        .params
        .map(|params| {
            arena.alloc_slice_try_fill_iter(params.iter().map(|p| -> Result<_> {
                let ty = infer::check(&mut ctx, def.phase, p.ty, universe)?;
                ctx.push_local(p.name, ty);
                Ok((p.name, ty))
            }))
        })
        .transpose()?;

    let core_ret_ty = infer::check(&mut ctx, def.phase, def.ret_ty, universe)?;

    match def.phase {
        Phase::Object => {
            let core_params = core_params.ok_or_else(|| {
                anyhow::anyhow!("object-phase constant definitions are not supported")
            })?;
            Ok(GlobalEntry::CodeFn {
                params: core_params,
                ret_ty: core_ret_ty,
            })
        }
        Phase::Meta => match core_params {
            Some(core_params) => {
                let pi = arena.alloc(core::Term::Pi(core::Pi {
                    params: core_params,
                    body_ty: core_ret_ty,
                }));
                Ok(GlobalEntry::Meta(pi))
            }
            None => Ok(GlobalEntry::Meta(core_ret_ty)),
        },
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

            let global = match def.phase {
                Phase::Object => {
                    let GlobalEntry::CodeFn { params, ret_ty } =
                        globals.get(&name).expect("signature missing from pass 1")
                    else {
                        unreachable!("Code def should have CodeFn entry")
                    };
                    for (param_name, param_ty) in *params {
                        ctx.push_local(param_name, param_ty);
                    }
                    let core_body = infer::check(&mut ctx, Phase::Object, def.body, ret_ty)
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
                Phase::Meta => {
                    let GlobalEntry::Meta(ty) =
                        globals.get(&name).expect("signature missing from pass 1")
                    else {
                        unreachable!("Meta def should have Meta entry")
                    };
                    // For function definitions (params present → Pi signature), push the Pi
                    // params into scope and check the body against pi.body_ty.
                    // This preserves `expected_term` through to match arm refinement —
                    // necessary for dependent return types.
                    // See also issue #74 — once fixed, this can be unified with the
                    // generic path below (no special casing for Pi needed).
                    let body = match (def.params, *ty) {
                        (Some(_), core::Term::Pi(pi)) => {
                            for (param_name, param_ty) in pi.params {
                                ctx.push_local(param_name, param_ty);
                            }
                            let core_body =
                                infer::check(&mut ctx, Phase::Meta, def.body, pi.body_ty)
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
