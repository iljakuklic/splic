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
    let d = &def.def;

    let ret_ty_ast = d
        .ret_ty
        .ok_or_else(|| anyhow::anyhow!("global `def` requires a return type annotation"))?;

    // Elaborate each param group in order, pushing params into scope so later groups
    // (and the return type) can depend on earlier ones.
    let mut all_groups: Vec<&'core [(&'names core::Name, &'core core::Term<'names, 'core>)]> =
        Vec::with_capacity(d.params.len());
    for group in d.params {
        let core_group = arena.alloc_slice_try_fill_iter(group.iter().map(|p| -> Result<_> {
            let ty = infer::check(&mut ctx, def.phase, p.ty, universe)?;
            ctx.push_local(p.name, ty);
            Ok((p.name, ty))
        }))?;
        all_groups.push(core_group);
    }

    let core_ret_ty = infer::check(&mut ctx, def.phase, ret_ty_ast, universe)?;

    match def.phase {
        Phase::Object => {
            let mut iter = all_groups.into_iter();
            match (iter.next(), iter.next()) {
                (None, _) => Err(anyhow::anyhow!(
                    "object-phase constant definitions are not supported"
                )),
                (Some(params), None) => Ok(GlobalEntry::CodeFn {
                    params,
                    ret_ty: core_ret_ty,
                }),
                _ => Err(anyhow::anyhow!(
                    "object-phase functions do not support curried parameter groups"
                )),
            }
        }
        Phase::Meta => {
            // Build nested Pi from inside out: fn(g0)(g1) -> T ≡ Pi{g0, Pi{g1, T}}
            let ty = all_groups.iter().rev().fold(core_ret_ty, |inner, &group| {
                arena.alloc(core::Term::Pi(core::Pi {
                    params: group,
                    body_ty: inner,
                }))
            });
            Ok(GlobalEntry::Meta(ty))
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
        let name = def.def.name;

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
            let name = def.def.name;
            let mut ctx = Ctx::new(arena, globals);

            let global = match def.phase {
                Phase::Object => {
                    let GlobalEntry::CodeFn { params, ret_ty } =
                        globals.get(&name).expect("signature missing from pass 1")
                    else {
                        unreachable!("Code def should have CodeFn entry")
                    };
                    let core_body = infer::check_with_params(
                        &mut ctx,
                        Phase::Object,
                        params,
                        def.def.body,
                        ret_ty,
                    )
                    .with_context(|| format!("in `{name}`"))?;
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
                    // For function definitions (1+ param groups → Pi signature), walk the Pi
                    // chain, push all params into scope, check the body against the innermost
                    // body_ty, then build nested Lam wrapping from inside out.
                    // This preserves `expected_term` through to match arm refinement —
                    // necessary for dependent return types.
                    // See also issue #74 — once fixed, this can be unified with the
                    // generic path below (no special casing for Pi needed).
                    let body = if def.def.params.is_empty() {
                        infer::check(&mut ctx, Phase::Meta, def.def.body, ty)
                            .with_context(|| format!("in `{name}`"))?
                    } else {
                        // Walk the Pi chain, collecting one Pi per param group and tracking
                        // the innermost body type (used to check the definition body).
                        let mut pi_chain: Vec<&'core core::Pi<'names, 'core>> = Vec::new();
                        let mut cur: &'core core::Term<'names, 'core> = ty;
                        let mut innermost_body_ty: &'core core::Term<'names, 'core> = ty;
                        for _ in def.def.params {
                            let core::Term::Pi(pi) = cur else {
                                unreachable!("expected Pi type for each param group")
                            };
                            pi_chain.push(pi);
                            innermost_body_ty = pi.body_ty;
                            cur = pi.body_ty;
                        }
                        // Push all params from all groups into scope.
                        for pi in &pi_chain {
                            for (n, t) in pi.params {
                                ctx.push_local(n, t);
                            }
                        }
                        let core_body =
                            infer::check(&mut ctx, Phase::Meta, def.def.body, innermost_body_ty)
                                .with_context(|| format!("in `{name}`"))?;
                        // Pop all params.
                        for pi in &pi_chain {
                            for _ in pi.params {
                                ctx.pop_local();
                            }
                        }
                        // Build nested Lam from inside out.
                        pi_chain.iter().rev().fold(core_body, |inner, pi| {
                            arena.alloc(core::Term::Lam(core::Lam {
                                params: pi.params,
                                body: inner,
                            }))
                        })
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
