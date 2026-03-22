//! Test helpers

use super::*;

/// Helper to create a test context with empty globals
pub fn test_ctx(arena: &bumpalo::Bump) -> Ctx<'_, '_> {
    static EMPTY: std::sync::OnceLock<HashMap<Name<'static>, core::FunSig<'static>>> =
        std::sync::OnceLock::new();
    let globals = EMPTY.get_or_init(HashMap::new);
    Ctx::new(arena, globals)
}

/// Helper to create a test context with a given globals table.
///
/// The caller must ensure `globals` outlives the returned `Ctx`.
pub fn test_ctx_with_globals<'core, 'globals>(
    arena: &'core bumpalo::Bump,
    globals: &'globals HashMap<Name<'core>, core::FunSig<'core>>,
) -> Ctx<'core, 'globals> {
    Ctx::new(arena, globals)
}

/// Helper: build a simple `FunSig` for a function `fn f() -> u64` (no params, meta phase).
pub fn sig_no_params_returns_u64() -> FunSig<'static> {
    let ret_ty = &core::Term::U64_META;
    FunSig {
        params: &[],
        ret_ty,
        phase: Phase::Meta,
    }
}

/// Helper: build a `FunSig` for `fn f(x: u32) -> u64`.
pub fn sig_one_param_returns_u64(core_arena: &bumpalo::Bump) -> FunSig<'_> {
    let u32_ty = &core::Term::U32_META;
    let u64_ty = &core::Term::U64_META;
    let param = core_arena.alloc(("x", u32_ty as &core::Term));
    FunSig {
        params: std::slice::from_ref(param),
        ret_ty: u64_ty,
        phase: Phase::Meta,
    }
}
