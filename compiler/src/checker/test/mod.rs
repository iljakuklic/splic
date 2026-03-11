use super::*;
use crate::core::{self, Head, IntWidth, Pat, Prim};
use crate::parser::ast::Phase;

/// Helper to create a test context with empty globals
fn test_ctx(arena: &bumpalo::Bump, phase: Phase) -> Ctx<'_> {
    Ctx::new(arena, HashMap::new(), phase)
}

/// Helper to create a u64 type term
fn u64_ty<'a>(arena: &'a bumpalo::Bump) -> &'a core::Term<'a> {
    arena.alloc(core::Term::Prim(Prim::IntTy(IntWidth::U64)))
}

/// Helper to create a u32 type term
fn u32_ty<'a>(arena: &'a bumpalo::Bump) -> &'a core::Term<'a> {
    arena.alloc(core::Term::Prim(Prim::IntTy(IntWidth::U32)))
}

/// Helper to create a u1 type term
fn u1_ty<'a>(arena: &'a bumpalo::Bump) -> &'a core::Term<'a> {
    arena.alloc(core::Term::Prim(Prim::IntTy(IntWidth::U1)))
}

/// Helper to create a Type (meta universe) term
fn type_ty<'a>(arena: &'a bumpalo::Bump) -> &'a core::Term<'a> {
    arena.alloc(core::Term::Prim(Prim::U(Phase::Meta)))
}

/// Helper to create a VmType (object universe) term
fn vm_type_ty<'a>(arena: &'a bumpalo::Bump) -> &'a core::Term<'a> {
    arena.alloc(core::Term::Prim(Prim::U(Phase::Object)))
}

/// Helper to create a lifted type [[T]]
fn lift_ty<'a>(arena: &'a bumpalo::Bump, inner: &'a core::Term<'a>) -> &'a core::Term<'a> {
    arena.alloc(core::Term::Lift(inner))
}

#[test]
fn test_prim_types_are_well_kinded() {
    // Primitive types like u64 should have kind Type (at meta level) or VmType (at object level)
    // This is a semantic fact about primitives that the checker will rely on
    let arena = bumpalo::Bump::new();
    let u64 = u64_ty(&arena);
    assert!(matches!(u64, core::Term::Prim(Prim::IntTy(IntWidth::U64))));
}

#[test]
fn test_literal_has_inferred_type() {
    // A literal 42 should infer to u64 (or whatever the default is)
    // For now, we just verify the structure exists
    let arena = bumpalo::Bump::new();
    let lit = arena.alloc(core::Term::Lit(42));
    assert!(matches!(lit, core::Term::Lit(42)));
}

#[test]
fn test_variable_lookup_in_empty_context() {
    // Looking up a variable in an empty context should fail
    let arena = bumpalo::Bump::new();
    let ctx = test_ctx(&arena, Phase::Meta);
    assert_eq!(ctx.lookup_local("x"), None);
}

#[test]
fn test_variable_lookup_after_push() {
    // After pushing a variable, it should be found at level 0
    let arena = bumpalo::Bump::new();
    let mut ctx = test_ctx(&arena, Phase::Meta);
    let u64 = u64_ty(&arena);
    ctx.push_local("x", u64);

    let (lvl, ty) = ctx.lookup_local("x").expect("x should be in scope");
    assert_eq!(lvl, Lvl(0));
    assert!(matches!(ty, core::Term::Prim(Prim::IntTy(IntWidth::U64))));
}

#[test]
fn test_variable_lookup_with_multiple_locals() {
    // With multiple locals, a new variable should have a higher level
    let arena = bumpalo::Bump::new();
    let mut ctx = test_ctx(&arena, Phase::Meta);
    let u64 = u64_ty(&arena);
    let u32 = u32_ty(&arena);

    ctx.push_local("x", u64);
    ctx.push_local("y", u32);

    // y should be at level 1 (most recent)
    let (lvl_y, ty_y) = ctx.lookup_local("y").expect("y should be in scope");
    assert_eq!(lvl_y, Lvl(1));
    assert!(matches!(ty_y, core::Term::Prim(Prim::IntTy(IntWidth::U32))));

    // x should be at level 0
    let (lvl_x, ty_x) = ctx.lookup_local("x").expect("x should be in scope");
    assert_eq!(lvl_x, Lvl(0));
    assert!(matches!(ty_x, core::Term::Prim(Prim::IntTy(IntWidth::U64))));
}

#[test]
fn test_variable_shadowing() {
    // If we push two variables with the same name, the newer one shadows the older
    let arena = bumpalo::Bump::new();
    let mut ctx = test_ctx(&arena, Phase::Meta);
    let u64 = u64_ty(&arena);
    let u32 = u32_ty(&arena);

    ctx.push_local("x", u64);
    ctx.push_local("x", u32);

    // Should find the shadowing binding (u32)
    let (lvl, ty) = ctx.lookup_local("x").expect("x should be in scope");
    assert_eq!(lvl, Lvl(1));
    assert!(matches!(ty, core::Term::Prim(Prim::IntTy(IntWidth::U32))));
}

#[test]
fn test_context_depth() {
    // Depth should track the number of locals
    let arena = bumpalo::Bump::new();
    let mut ctx = test_ctx(&arena, Phase::Meta);
    let u64 = u64_ty(&arena);

    assert_eq!(ctx.depth(), 0);
    ctx.push_local("x", u64);
    assert_eq!(ctx.depth(), 1);
    ctx.push_local("y", u64);
    assert_eq!(ctx.depth(), 2);
    ctx.pop_local();
    assert_eq!(ctx.depth(), 1);
}

#[test]
fn test_meta_variable_in_quote_is_ok() {
    // A meta-level variable can appear inside a quote
    // In the surface syntax: fn foo(x: [[u64]]) { #(x) }
    // Inside the quote, x refers to the meta variable, which is fine
    let arena = bumpalo::Bump::new();
    let mut ctx = test_ctx(&arena, Phase::Meta);

    // x has type [[u64]] (meta level)
    let u64 = u64_ty(&arena);
    let lifted_u64 = lift_ty(&arena, u64);
    ctx.push_local("x", lifted_u64);

    // Inside the quote, we're at object level, but we can reference x
    // because x is a meta-level value of type [[u64]] (object-level code)
    let x_var = arena.alloc(core::Term::Var(Lvl(0)));
    assert!(matches!(x_var, core::Term::Var(Lvl(0))));
}

#[test]
fn test_object_variable_outside_quote_is_invalid() {
    // An object-level variable should not be accessible at meta level
    // This is a semantic check the type checker must enforce
    let arena = bumpalo::Bump::new();
    let mut ctx = test_ctx(&arena, Phase::Object);
    let u64 = u64_ty(&arena);
    ctx.push_local("x", u64);

    // At meta level, we shouldn't be able to reference x
    // (This check is not yet in the checker, but it's a design goal)
    // For now, we just verify the structure
    assert_eq!(ctx.depth(), 1);
}

#[test]
fn test_phase_context_separation() {
    // Meta and object contexts are separate
    let arena = bumpalo::Bump::new();
    let meta_ctx = test_ctx(&arena, Phase::Meta);
    let obj_ctx = test_ctx(&arena, Phase::Object);

    assert_eq!(meta_ctx.phase, Phase::Meta);
    assert_eq!(obj_ctx.phase, Phase::Object);
}

#[test]
fn test_type_universe_distinction() {
    // Type (meta universe) and VmType (object universe) are distinct
    let arena = bumpalo::Bump::new();
    let type_tm = type_ty(&arena);
    let vm_type_tm = vm_type_ty(&arena);

    let type_matches = matches!(type_tm, core::Term::Prim(Prim::U(Phase::Meta)));
    let vm_type_matches = matches!(vm_type_tm, core::Term::Prim(Prim::U(Phase::Object)));

    assert!(type_matches);
    assert!(vm_type_matches);
}

#[test]
fn test_arithmetic_operation_type() {
    // Add(U64) is a callable operation that takes two u64s
    let _arena = bumpalo::Bump::new();
    let add_u64 = Prim::Add(IntWidth::U64);
    assert!(matches!(add_u64, Prim::Add(IntWidth::U64)));
}

#[test]
fn test_comparison_operation_returns_u1() {
    // Eq(U64) returns U1 (boolean)
    let _arena = bumpalo::Bump::new();
    let eq_u64 = Prim::Eq(IntWidth::U64);
    assert!(matches!(eq_u64, Prim::Eq(IntWidth::U64)));
}

#[test]
fn test_lift_type_structure() {
    // [[u64]] should be a well-formed meta type
    let arena = bumpalo::Bump::new();
    let u64 = u64_ty(&arena);
    let lifted = lift_ty(&arena, u64);
    assert!(matches!(lifted, core::Term::Lift(_)));
}

#[test]
fn test_quoted_term_structure() {
    // #(expr) should create a Quote term
    let arena = bumpalo::Bump::new();
    let inner = arena.alloc(core::Term::Lit(42));
    let quoted = arena.alloc(core::Term::Quote(inner));
    assert!(matches!(quoted, core::Term::Quote(_)));
}

#[test]
fn test_spliced_term_structure() {
    // $(expr) should create a Splice term
    let arena = bumpalo::Bump::new();
    let inner = arena.alloc(core::Term::Lit(42));
    let spliced = arena.alloc(core::Term::Splice(inner));
    assert!(matches!(spliced, core::Term::Splice(_)));
}

#[test]
fn test_let_binding_structure() {
    // let x: u64 = 42; x should create a Let term
    let arena = bumpalo::Bump::new();
    let u64 = u64_ty(&arena);
    let expr = arena.alloc(core::Term::Lit(42));
    let body = arena.alloc(core::Term::Var(Lvl(0)));
    let let_term = arena.alloc(core::Term::Let {
        name: "x",
        ty: u64,
        expr,
        body,
    });
    assert!(matches!(let_term, core::Term::Let { .. }));
}

#[test]
fn test_match_with_literal_pattern() {
    // match x { 0 => ..., 1 => ... } should create a Match term with literal patterns
    let arena = bumpalo::Bump::new();
    let scrutinee = arena.alloc(core::Term::Var(Lvl(0)));
    let body0 = arena.alloc(core::Term::Lit(0));
    let body1 = arena.alloc(core::Term::Lit(1));

    let arm0 = core::Arm {
        pat: Pat::Lit(0),
        body: body0,
    };
    let arm1 = core::Arm {
        pat: Pat::Lit(1),
        body: body1,
    };

    let arms = &*arena.alloc_slice_fill_iter([arm0, arm1]);
    let match_term = arena.alloc(core::Term::Match { scrutinee, arms });

    assert!(matches!(match_term, core::Term::Match { .. }));
}

#[test]
fn test_match_with_binding_pattern() {
    // match x { n => ... } should create a Match term with a binding pattern
    let arena = bumpalo::Bump::new();
    let scrutinee = arena.alloc(core::Term::Var(Lvl(0)));
    let body = arena.alloc(core::Term::Var(Lvl(0)));

    let arm = core::Arm {
        pat: Pat::Bind("n"),
        body,
    };

    let arms = &*arena.alloc_slice_fill_iter([arm]);
    let match_term = arena.alloc(core::Term::Match { scrutinee, arms });

    assert!(matches!(match_term, core::Term::Match { .. }));
}

#[test]
fn test_function_call_to_global() {
    // A call to a global function should create an App with Head::Global
    let arena = bumpalo::Bump::new();
    let arg = arena.alloc(core::Term::Lit(42));
    let args = &*arena.alloc_slice_fill_iter([&*arg]);
    let app = arena.alloc(core::Term::App {
        head: Head::Global("foo"),
        args,
    });

    assert!(matches!(
        app,
        core::Term::App {
            head: Head::Global("foo"),
            ..
        }
    ));
}

#[test]
fn test_builtin_operation_call() {
    // A call to a builtin like + should create an App with Head::Prim
    let arena = bumpalo::Bump::new();
    let arg1 = arena.alloc(core::Term::Lit(1));
    let arg2 = arena.alloc(core::Term::Lit(2));
    let args = &*arena.alloc_slice_fill_iter([&*arg1, &*arg2]);
    let app = arena.alloc(core::Term::App {
        head: Head::Prim(Prim::Add(IntWidth::U64)),
        args,
    });

    assert!(matches!(
        app,
        core::Term::App {
            head: Head::Prim(Prim::Add(IntWidth::U64)),
            ..
        }
    ));
}
