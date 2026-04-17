use super::Term;

/// Alpha-equality: structural equality ignoring `param_name` fields in Pi/Lam.
pub fn alpha_eq(a: &Term<'_, '_>, b: &Term<'_, '_>) -> bool {
    // Fast path: pointer equality
    if std::ptr::eq(a, b) {
        return true;
    }
    match (a, b) {
        (Term::Var(l1), Term::Var(l2)) => l1 == l2,
        (Term::Prim(p1), Term::Prim(p2)) => p1 == p2,
        (Term::Lit(n1, t1), Term::Lit(n2, t2)) => n1 == n2 && t1 == t2,
        (Term::Global(n1), Term::Global(n2)) => n1 == n2,
        (Term::App(a1), Term::App(a2)) => {
            alpha_eq(a1.func, a2.func)
                && a1.args.len() == a2.args.len()
                && a1
                    .args
                    .iter()
                    .zip(a2.args.iter())
                    .all(|(x, y)| alpha_eq(x, y))
        }
        (Term::Pi(p1), Term::Pi(p2)) => {
            p1.params.len() == p2.params.len()
                && p1
                    .params
                    .iter()
                    .zip(p2.params.iter())
                    .all(|((_, t1), (_, t2))| alpha_eq(t1, t2))
                && alpha_eq(p1.body_ty, p2.body_ty)
        }
        (Term::Lam(l1), Term::Lam(l2)) => {
            l1.params.len() == l2.params.len()
                && l1
                    .params
                    .iter()
                    .zip(l2.params.iter())
                    .all(|((_, t1), (_, t2))| alpha_eq(t1, t2))
                && alpha_eq(l1.body, l2.body)
        }
        (Term::Lift(i1), Term::Lift(i2))
        | (Term::Quote(i1), Term::Quote(i2))
        | (Term::Splice(i1), Term::Splice(i2)) => alpha_eq(i1, i2),
        (Term::Let(l1), Term::Let(l2)) => {
            alpha_eq(l1.ty, l2.ty) && alpha_eq(l1.expr, l2.expr) && alpha_eq(l1.body, l2.body)
        }
        (Term::Match(m1), Term::Match(m2)) => {
            alpha_eq(m1.scrutinee, m2.scrutinee)
                && m1.arms.len() == m2.arms.len()
                && m1
                    .arms
                    .iter()
                    .zip(m2.arms.iter())
                    .all(|(a, b)| a.pat == b.pat && alpha_eq(a.body, b.body))
        }
        _ => false,
    }
}
