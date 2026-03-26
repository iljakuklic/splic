use super::{Arm, Lam, Lvl, Pi, Term};

/// Substitute `replacement` for `Var(target)` in `term`.
pub fn subst<'a>(
    arena: &'a bumpalo::Bump,
    term: &'a Term<'a>,
    target: Lvl,
    replacement: &'a Term<'a>,
) -> &'a Term<'a> {
    match term {
        Term::Var(lvl) if *lvl == target => replacement,
        Term::Var(_) | Term::Prim(_) | Term::Lit(..) | Term::Global(_) => term,

        Term::App(app) => {
            let new_func = subst(arena, app.func, target, replacement);
            let new_args = arena.alloc_slice_fill_iter(
                app.args
                    .iter()
                    .map(|arg| subst(arena, arg, target, replacement)),
            );
            arena.alloc(Term::new_app(new_func, new_args))
        }

        Term::Pi(pi) => {
            let new_params = arena.alloc_slice_fill_iter(
                pi.params.iter().map(|&(name, ty)| (name, subst(arena, ty, target, replacement))),
            );
            let new_body_ty = subst(arena, pi.body_ty, target, replacement);
            arena.alloc(Term::Pi(Pi { params: new_params, body_ty: new_body_ty }))
        }

        Term::Lam(lam) => {
            let new_params = arena.alloc_slice_fill_iter(
                lam.params.iter().map(|&(name, ty)| (name, subst(arena, ty, target, replacement))),
            );
            let new_body = subst(arena, lam.body, target, replacement);
            arena.alloc(Term::Lam(Lam { params: new_params, body: new_body }))
        }

        Term::Lift(inner) => {
            let new_inner = subst(arena, inner, target, replacement);
            arena.alloc(Term::Lift(new_inner))
        }
        Term::Quote(inner) => {
            let new_inner = subst(arena, inner, target, replacement);
            arena.alloc(Term::Quote(new_inner))
        }
        Term::Splice(inner) => {
            let new_inner = subst(arena, inner, target, replacement);
            arena.alloc(Term::Splice(new_inner))
        }

        Term::Let(let_) => {
            let new_ty = subst(arena, let_.ty, target, replacement);
            let new_expr = subst(arena, let_.expr, target, replacement);
            let new_body = subst(arena, let_.body, target, replacement);
            arena.alloc(Term::new_let(let_.name, new_ty, new_expr, new_body))
        }

        Term::Match(match_) => {
            let new_scrutinee = subst(arena, match_.scrutinee, target, replacement);
            let new_arms = arena.alloc_slice_fill_iter(match_.arms.iter().map(|arm| Arm {
                pat: arm.pat.clone(),
                body: subst(arena, arm.body, target, replacement),
            }));
            arena.alloc(Term::new_match(new_scrutinee, new_arms))
        }
    }
}
