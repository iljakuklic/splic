//! Staging-pass properties checked by the snapshot runner after each successful
//! stage snapshot.

use crate::common::global_alpha_eq;
use bumpalo::Bump;
use splic_compiler::{
    core::{Global, Program, Term},
    staging::unstage_program,
};

/// Check the staging pass's correctness properties on a successfully staged program:
///
/// - **Stability**: a `code def` containing no splices stages to itself.
/// - **Idempotence**: staging the staged output again changes nothing (it is splice-free,
///   so this is stability applied to every program in the suite).
pub fn check_staging_properties(core: &Program<'_, '_>, staged: &Program<'_, '_>) {
    for def in core
        .defs
        .iter()
        .filter(|d| !global_contains_splice(&d.global))
    {
        if let Global::Meta(_) = def.global {
            continue;
        }
        let out = staged
            .defs
            .iter()
            .find(|d| d.name == def.name)
            .unwrap_or_else(|| panic!("code def `{}` missing from staged output", def.name));
        assert!(
            global_alpha_eq(&def.global, &out.global),
            "stability: splice-free code def changed by staging\n\
             before:\n{def}\nafter:\n{out}",
        );
    }

    let arena = Bump::new();
    let restaged = unstage_program(&arena, staged).expect("restaging staged output failed");
    assert_eq!(
        staged.defs.len(),
        restaged.defs.len(),
        "restaging changed def count"
    );
    for (a, b) in staged.defs.iter().zip(restaged.defs) {
        assert!(
            a.name == b.name && global_alpha_eq(&a.global, &b.global),
            "idempotence: restaging changed a def\nstaged:\n{a}\nrestaged:\n{b}",
        );
    }
}

fn global_contains_splice(global: &Global<'_, '_>) -> bool {
    match global {
        Global::Meta(m) => contains_splice(m.ty) || contains_splice(m.body),
        Global::CodeFn(f) => {
            f.params.iter().any(|(_, t)| contains_splice(t))
                || contains_splice(f.ret_ty)
                || contains_splice(f.body)
        }
        Global::CodeConst(c) => contains_splice(c.ty) || contains_splice(c.body),
    }
}

/// Whether `term` contains a splice anywhere. The match is exhaustive on purpose, so a
/// new `Term` variant must be handled here.
fn contains_splice(term: &Term<'_, '_>) -> bool {
    match term {
        Term::Splice(_) => true,
        Term::Var(_) | Term::Prim(_) | Term::Lit(..) | Term::Global(_) => false,
        Term::App(app) => contains_splice(app.func) || app.args.iter().any(|a| contains_splice(a)),
        Term::Pi(pi) => {
            pi.params.iter().any(|(_, t)| contains_splice(t)) || contains_splice(pi.body_ty)
        }
        Term::Lam(lam) => {
            lam.params.iter().any(|(_, t)| contains_splice(t)) || contains_splice(lam.body)
        }
        Term::Lift(t) | Term::Quote(t) => contains_splice(t),
        Term::Let(l) => contains_splice(l.ty) || contains_splice(l.expr) || contains_splice(l.body),
        Term::Match(m) => {
            contains_splice(m.scrutinee) || m.arms.iter().any(|arm| contains_splice(arm.body))
        }
    }
}
