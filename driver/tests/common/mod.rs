//! Helpers shared by the driver's integration test binaries.

use splic_compiler::core::{Global, alpha_eq};

/// Alpha-equality of two object-level globals.
pub fn global_alpha_eq(a: &Global<'_, '_>, b: &Global<'_, '_>) -> bool {
    match (a, b) {
        (Global::CodeFn(f), Global::CodeFn(g)) => {
            f.params.len() == g.params.len()
                && f.params
                    .iter()
                    .zip(g.params)
                    .all(|((_, t1), (_, t2))| alpha_eq(t1, t2))
                && alpha_eq(f.ret_ty, g.ret_ty)
                && alpha_eq(f.body, g.body)
        }
        (Global::CodeConst(c), Global::CodeConst(d)) => {
            alpha_eq(c.ty, d.ty) && alpha_eq(c.body, d.body)
        }
        _ => false,
    }
}
