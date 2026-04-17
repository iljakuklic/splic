//! Test helpers

use super::*;
use crate::checker::ctx::GlobalEntry;

/// Helper to create a test context with empty globals
pub fn test_ctx(arena: &bumpalo::Bump) -> Ctx<'_, '_, '_> {
    static EMPTY: std::sync::OnceLock<
        HashMap<&'static Name, GlobalEntry<'static, 'static>>,
    > = std::sync::OnceLock::new();
    let globals = EMPTY.get_or_init(HashMap::new);
    Ctx::new(arena, globals)
}

/// Helper to create a test context with a given globals table.
///
/// The caller must ensure `globals` outlives the returned `Ctx`.
pub fn test_ctx_with_globals<'names, 'core, 'globals>(
    arena: &'core bumpalo::Bump,
    globals: &'globals HashMap<&'names Name, GlobalEntry<'names, 'core>>,
) -> Ctx<'names, 'core, 'globals> {
    Ctx::new(arena, globals)
}

/// Helper: build a `GlobalEntry::Meta` for `fn f() -> u64` (no params, meta phase).
pub fn sig_no_params_returns_u64(arena: &bumpalo::Bump) -> GlobalEntry<'_, '_> {
    GlobalEntry::Meta(arena.alloc(core::Term::Pi(Pi {
        params: &[],
        body_ty: &core::Term::U64_META,
    })))
}

/// Helper: build a `GlobalEntry::Meta` for `fn f(x: u32) -> u64`.
pub fn sig_one_param_returns_u64(arena: &bumpalo::Bump) -> GlobalEntry<'_, '_> {
    let params =
        arena.alloc_slice_fill_iter([(core::Name::new("x"), &core::Term::U32_META as &core::Term)]);
    GlobalEntry::Meta(arena.alloc(core::Term::Pi(Pi {
        params,
        body_ty: &core::Term::U64_META,
    })))
}
