//! Checker behaviour tests — Metaprogramming (Lift, Quote, Splice)

use super::*;

// `[[u64]]` is a well-formed meta type: `infer` returns `Type`.
#[test]
fn infer_lift_of_object_type_returns_type_universe() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();
    let mut ctx = test_ctx(&core_arena);

    // `[[u64]]` in surface AST: `Lift(Var("u64"))`
    let inner = src_arena.alloc(ast::Term::Var(ast::Name::new("u64")));
    let term = src_arena.alloc(ast::Term::Lift(inner));

    // Elaborated at meta phase: type of [[u64]] is Type (meta universe)
    let (_, ty_val) = infer(&mut ctx, Phase::Meta, term).expect("should infer");
    assert!(matches!(ty_val, value::Value::Prim(Prim::U(Phase::Meta))));
}

// `[[u64]]` is illegal at object phase — Lift is only meaningful in meta context.
#[test]
fn infer_lift_at_object_phase_fails() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();
    let mut ctx = test_ctx(&core_arena);

    let inner = src_arena.alloc(ast::Term::Var(ast::Name::new("u64")));
    let term = src_arena.alloc(ast::Term::Lift(inner));

    // Lift is only legal at meta phase.
    assert!(infer(&mut ctx, Phase::Object, term).is_err());
}

// Lifting a non-type value must fail.
#[test]
fn infer_lift_of_non_type_fails() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();
    let mut ctx = test_ctx(&core_arena);

    // Push a local `x: u32` (a value, not a type) then write `[[x]]`
    let u32_ty = &core::Term::U32_META;
    ctx.push_local(core::Name::new("x"), u32_ty);

    let inner = src_arena.alloc(ast::Term::Var(ast::Name::new("x")));
    let term = src_arena.alloc(ast::Term::Lift(inner));

    assert!(infer(&mut ctx, Phase::Meta, term).is_err());
}

// ---------------------------------------------------------------------------
// Checker behaviour tests — Quote
// ---------------------------------------------------------------------------

// `#(f())` where `f: () -> u64` at the object phase infers as `[[u64]]` at meta.
#[test]
fn infer_quote_of_global_call_returns_lifted_type() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();

    // `code fn f() -> u64` — object-phase function
    let u64_ty_core = &core::Term::U64_OBJ;
    let mut globals = HashMap::new();
    globals.insert(
        Name::new("f"),
        &*core_arena.alloc(core::Term::Pi(Pi {
            params: &[],
            body_ty: u64_ty_core,
            phase: Phase::Object,
        })),
    );
    let mut ctx = test_ctx_with_globals(&core_arena, &globals);

    // Surface: `#(f())`
    let inner = src_arena.alloc(ast::Term::App {
        func: FunName::Term(src_arena.alloc(ast::Term::Var(ast::Name::new("f")))),
        args: &[],
    });
    let term = src_arena.alloc(ast::Term::Quote(inner));

    // Checked at meta phase; result type should be [[u64]]
    let (core_term, ty_val) = infer(&mut ctx, Phase::Meta, term).expect("should infer");
    assert!(matches!(core_term, core::Term::Quote(_)));
    // Type is Lift(u64)
    assert!(matches!(ty_val, value::Value::Lift(_)));
}

// `#(...)` at object phase is illegal — Quote is only meaningful in meta context.
#[test]
fn infer_quote_at_object_phase_fails() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();

    let u64_ty_core = &core::Term::U64_OBJ;
    let mut globals = HashMap::new();
    globals.insert(
        Name::new("f"),
        &*core_arena.alloc(core::Term::Pi(Pi {
            params: &[],
            body_ty: u64_ty_core,
            phase: Phase::Object,
        })),
    );
    let mut ctx = test_ctx_with_globals(&core_arena, &globals);

    let inner = src_arena.alloc(ast::Term::App {
        func: FunName::Term(src_arena.alloc(ast::Term::Var(ast::Name::new("f")))),
        args: &[],
    });
    let term = src_arena.alloc(ast::Term::Quote(inner));

    // Quote is only legal at meta phase.
    assert!(infer(&mut ctx, Phase::Object, term).is_err());
}

// `#(42)` — literal inside quote is not inferable, so the whole quote is not inferrable.
#[test]
fn infer_quote_of_literal_fails() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();
    let mut ctx = test_ctx(&core_arena);

    let inner = src_arena.alloc(ast::Term::Lit(42));
    let term = src_arena.alloc(ast::Term::Quote(inner));

    assert!(infer(&mut ctx, Phase::Meta, term).is_err());
}

// Inside `#(...)` the phase is object, so a meta-only construct would be invalid.
// We can test this by verifying that checking `#(x)` where x: [[u64]] succeeds
// (meta var, used via splice), and that the inner term is indeed checked at object phase.
#[test]
fn check_quote_switches_to_object_phase() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();
    let mut ctx = test_ctx(&core_arena);

    // x : [[u64]] — meta variable holding object code; Lift contains an object-phase type.
    let u64_obj = core::Term::int_ty(IntWidth::U64, Phase::Object);
    let lifted = ctx.lift_ty(u64_obj);
    ctx.push_local(core::Name::new("x"), lifted);

    // `#($(x))` — splice x inside a quote; type should be [[u64]]
    let x = src_arena.alloc(ast::Term::Var(ast::Name::new("x")));
    let splice_x = src_arena.alloc(ast::Term::Splice(x));
    let term = src_arena.alloc(ast::Term::Quote(splice_x));

    // [[u64(object)]] as the expected meta type
    let expected = core_arena.alloc(core::Term::Lift(&core::Term::U64_OBJ));

    let result = check(&mut ctx, Phase::Meta, term, expected).expect("should check");
    assert!(matches!(result, core::Term::Quote(_)));
}

// ---------------------------------------------------------------------------
// Checker behaviour tests — Splice
// ---------------------------------------------------------------------------

// `$(x)` where `x: [[u64]]` infers as `u64` at object phase.
#[test]
fn infer_splice_of_lifted_var_returns_inner_type() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();
    let mut ctx = test_ctx(&core_arena);

    let u64_ty = &core::Term::U64_META;
    let lifted = ctx.lift_ty(u64_ty);
    ctx.push_local(core::Name::new("x"), lifted); // x: [[u64]]

    let x = src_arena.alloc(ast::Term::Var(ast::Name::new("x")));
    let term = src_arena.alloc(ast::Term::Splice(x));

    // splice is checked at object phase
    let (core_term, ty_val) = infer(&mut ctx, Phase::Object, term).expect("should infer");
    assert!(matches!(core_term, core::Term::Splice(_)));
    assert!(matches!(
        ty_val,
        value::Value::Prim(Prim::IntTy(IntType {
            width: IntWidth::U64,
            ..
        }))
    ));
}

// `$(x)` at meta phase is illegal — Splice is only meaningful in object context.
#[test]
fn infer_splice_at_meta_phase_fails() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();
    let mut ctx = test_ctx(&core_arena);

    let u64_ty = &core::Term::U64_META;
    let lifted = ctx.lift_ty(u64_ty);
    ctx.push_local(core::Name::new("x"), lifted); // x: [[u64]]

    let x = src_arena.alloc(ast::Term::Var(ast::Name::new("x")));
    let term = src_arena.alloc(ast::Term::Splice(x));

    // Splice is only legal at object phase.
    assert!(infer(&mut ctx, Phase::Meta, term).is_err());
}

// `$(x)` where `x: u32` at meta phase splices a meta integer into object context.
#[test]
fn infer_splice_of_meta_int_succeeds() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();
    let mut ctx = test_ctx(&core_arena);

    // x: u32 at meta phase
    let u32_meta = &core::Term::U32_META;
    ctx.push_local(core::Name::new("x"), u32_meta);

    let x = src_arena.alloc(ast::Term::Var(ast::Name::new("x")));
    let term = src_arena.alloc(ast::Term::Splice(x));

    // $(x) at object phase: result type is u32 at object phase.
    let (core_term, ty_val) = infer(&mut ctx, Phase::Object, term).expect("should infer");
    assert!(matches!(core_term, core::Term::Splice(_)));
    assert!(matches!(
        ty_val,
        value::Value::Prim(Prim::IntTy(IntType {
            width: IntWidth::U32,
            phase: Phase::Object,
        }))
    ));
}

// `$(42)` — literal inside splice has no type, so infer fails.
#[test]
fn infer_splice_of_literal_fails() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();
    let mut ctx = test_ctx(&core_arena);

    let inner = src_arena.alloc(ast::Term::Lit(42));
    let term = src_arena.alloc(ast::Term::Splice(inner));

    assert!(infer(&mut ctx, Phase::Object, term).is_err());
}

// `$(x)` where `x: u64` at meta phase succeeds — meta integers can be spliced.
// `$(x)` where `x: Type` (not an integer, not lifted) must fail.
#[test]
fn infer_splice_of_non_lifted_non_int_var_fails() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();
    let mut ctx = test_ctx(&core_arena);

    let type_ty = &core::Term::TYPE; // Type (meta universe), not an integer or [[T]]
    ctx.push_local(core::Name::new("x"), type_ty);

    let x = src_arena.alloc(ast::Term::Var(ast::Name::new("x")));
    let term = src_arena.alloc(ast::Term::Splice(x));

    assert!(infer(&mut ctx, Phase::Object, term).is_err());
}
