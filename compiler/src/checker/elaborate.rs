use std::collections::HashMap;

use anyhow::{Context as _, Result, ensure};

use crate::core::{self, Pi};
use crate::parser::ast;

use super::Ctx;
use super::infer;

/// Elaborate one definition's type annotation into the globals table entry (`&Term`).
///
/// For function defs (`def.ty` is `ast::Term::Pi`), the params are elaborated with
/// `def.phase` so that `code def` object-level Pi types are handled correctly.
/// For constant defs (`def.ty` is anything else), the type is elaborated in the meta phase.
fn elaborate_sig<'names, 'ast, 'core>(
    arena: &'core bumpalo::Bump,
    def: &ast::GlobalDef<'names, 'ast>,
) -> Result<&'core core::Term<'names, 'core>> {
    let empty_globals: HashMap<&'names core::Name, &'core core::Term<'names, 'core>> =
        HashMap::new();
    let mut ctx = Ctx::new(arena, &empty_globals);

    match def.ty {
        ast::Term::Pi { params, ret_ty } => {
            // Function definition: manually elaborate params with def.phase so that
            // `code def` produces an object-level Pi.
            let elaborated_params: &'core [(&'names core::Name, &'core core::Term<'names, 'core>)] =
                arena.alloc_slice_try_fill_iter(params.iter().map(|p| -> Result<_> {
                    let (param_ty, _) = infer::infer(&mut ctx, def.phase, p.ty)?;
                    ctx.push_local(p.name, param_ty);
                    Ok((p.name, param_ty))
                }))?;

            let body_ty = infer::check(
                &mut ctx,
                def.phase,
                ret_ty,
                core::Term::universe(def.phase),
            )?;

            Ok(arena.alloc(core::Term::Pi(Pi {
                params: elaborated_params,
                body_ty,
                phase: def.phase,
            })))
        }
        _ => {
            // Constant definition: type is always meta-level.
            ensure!(
                def.phase.is_meta(),
                "`code def` requires a parameter list; constants are meta-level only"
            );
            let (ty_term, _) = infer::infer(&mut ctx, def.phase, def.ty)?;
            Ok(ty_term)
        }
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
            "duplicate definition name `{name}`"
        );

        let ty = elaborate_sig(arena, def).with_context(|| format!("in definition `{name}`"))?;

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

            // Build a fresh context borrowing the stack-owned globals map.
            let mut ctx = Ctx::new(arena, globals);

            let body = if let core::Term::Pi(pi) = ty {
                if pi.phase.is_meta() {
                    // Meta function: `def.expr` is a `Lam` (parser desugaring).
                    // The Lam checker pushes its own params, so we do NOT push them here.
                    // Unwrap the resulting core Lam to extract its inner body —
                    // `core::GlobalDef.body` stores the body expression, not the Lam wrapper.
                    let lam_term = infer::check(&mut ctx, pi.phase, def.expr, ty)
                        .with_context(|| format!("in definition `{name}`"))?;
                    match lam_term {
                        core::Term::Lam(l) => l.body,
                        _ => unreachable!(
                            "check(Lam, Pi) must produce a Lam (checker invariant)"
                        ),
                    }
                } else {
                    // Object function (`code def`): `def.expr` is the raw body (no Lam).
                    // Push parameters as locals so the body can reference them.
                    for (pname, pty) in pi.params {
                        ctx.push_local(pname, pty);
                    }
                    infer::check(&mut ctx, pi.phase, def.expr, pi.body_ty)
                        .with_context(|| format!("in definition `{name}`"))?
                }
            } else {
                // Constant definition: check body directly against the declared type.
                infer::check(&mut ctx, def.phase, def.expr, ty)
                    .with_context(|| format!("in definition `{name}`"))?
            };

            Ok(core::GlobalDef { name, ty, body })
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
