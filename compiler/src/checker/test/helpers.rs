//! Test helpers

use super::*;

/// Helper to create a test context with empty globals
pub fn test_ctx(arena: &bumpalo::Bump) -> Ctx<'_, '_> {
    static EMPTY: std::sync::OnceLock<HashMap<Name<'static>, &'static core::Pi<'static>>> =
        std::sync::OnceLock::new();
    let globals = EMPTY.get_or_init(HashMap::new);
    Ctx::new(arena, globals)
}

/// Helper to create a test context with a given globals table.
///
/// The caller must ensure `globals` outlives the returned `Ctx`.
pub fn test_ctx_with_globals<'core, 'globals>(
    arena: &'core bumpalo::Bump,
    globals: &'globals HashMap<Name<'core>, &'core core::Pi<'core>>,
) -> Ctx<'core, 'globals> {
    Ctx::new(arena, globals)
}

/// Helper: build a Pi for a function `fn f() -> u64` (no params, meta phase).
pub fn sig_no_params_returns_u64(arena: &bumpalo::Bump) -> &core::Pi<'_> {
    arena.alloc(Pi {
        params: &[],
        body_ty: &core::Term::U64_META,
        phase: Phase::Meta,
    })
}

/// Helper: build a Pi for `fn f(x: u32) -> u64`.
pub fn sig_one_param_returns_u64(arena: &bumpalo::Bump) -> &core::Pi<'_> {
    let params = arena.alloc_slice_fill_iter([("x", &core::Term::U32_META as &core::Term)]);
    arena.alloc(Pi {
        params,
        body_ty: &core::Term::U64_META,
        phase: Phase::Meta,
    })
}
