//! Reproducers for known bugs, found by the 2LTT conformance audit
//! (`docs/bs/2ltt_conformance_2026_07_17.md`).
//!
//! Each test pins the **current, incorrect** behaviour so the reproducer is
//! not lost. When the referenced issue is fixed, the corresponding test here
//! starts failing — at that point, invert it into a proper regression test
//! (typically a `type_error` snapshot asserting a clean elaboration error).

#![allow(clippy::unwrap_used, reason = "test code")]
#![allow(
    clippy::tests_outside_test_module,
    reason = "integration test binary under tests/ is already test-only"
)]

use bumpalo::Bump;
use splic_compiler::{
    checker::elaborate_program, lexer::Lexer, parser::Parser, staging::unstage_program,
};

/// Run lex → parse → elaborate → stage, panicking on any phase failure.
/// Returns the pretty-printed staged program.
fn stage(src: &str) -> String {
    let arena = Bump::new();
    let tokens: Vec<_> = Lexer::new(src, &arena)
        .collect::<Result<_, _>>()
        .expect("lex");
    let program = Parser::new(tokens.into_iter().map(Ok), &arena)
        .parse_program()
        .expect("parse");
    let core = elaborate_program(&arena, &program).expect("elaborate");
    let staged = unstage_program(&arena, &core).expect("stage");
    format!("{staged}")
}

/// Run lex → parse → elaborate, expecting elaboration to fail.
/// Returns the full error context chain.
fn elaborate_error(src: &str) -> String {
    let arena = Bump::new();
    let tokens: Vec<_> = Lexer::new(src, &arena)
        .collect::<Result<_, _>>()
        .expect("lex");
    let program = Parser::new(tokens.into_iter().map(Ok), &arena)
        .parse_program()
        .expect("parse");
    let err = elaborate_program(&arena, &program).expect_err("elaboration unexpectedly succeeded");
    format!("{err:#}")
}

/// Issue #74: a stuck `match` evaluates to an arm-less `Value::App`, so any
/// two dependent match types over the same scrutinee are convertible — here
/// `bad` swaps the arms of `orig`'s return type and is still accepted. The
/// staged output puts the value 42 at type `u0`, which only holds 0.
///
/// Once #74 is fixed, elaboration must reject `bad` with a type mismatch and
/// this test should move to a `type_error` snapshot.
#[test]
fn issue_74_stuck_match_conversion_accepts_unsound_program() {
    let src = "
        def orig(b: u1) -> (match b { 0 => u0, 1 => u16 }) = match b { 0 => 0, 1 => 42 };
        def bad(b: u1) -> (match b { 0 => u16, 1 => u0 }) = orig(b);
        code def boom() -> u0 = { $(bad(1)) };
    ";
    let staged = stage(src);
    assert!(
        staged.contains("42_u0"),
        "expected the unsound `42 : u0` literal in staged output, got:\n{staged}"
    );
}

/// Issue #27: match scrutinee phases are never checked, so a meta-phase
/// `match` on an object-phase variable elaborates and then panics in the
/// staging pass (the naive rendering of the CFTT case-split-on-code pattern).
///
/// Once #27 is fixed, elaboration must reject this with a phase error and
/// this test should move to a `type_error` snapshot.
#[test]
#[should_panic(expected = "object variable")]
fn issue_27_meta_match_on_object_var_panics_in_staging() {
    let src = "
        code def f(x: u64) -> u64 = {
            $( match x { 0 => #(1), _ => #(2) } )
        };
    ";
    let _ = stage(src);
}

/// Issue #27 (dual direction): an object-phase `match` on a meta-phase
/// variable elaborates and then panics in the staging pass.
#[test]
#[should_panic(expected = "referenced in object context")]
fn issue_27_object_match_on_meta_var_panics_in_staging() {
    let src = "
        def g(n: u64) -> [[u64]] = #( match n { 0 => 1, _ => 2 } );
        code def h() -> u64 = { $(g(3)) };
    ";
    let _ = stage(src);
}

/// Issue #48: no meta-level computation of primitives in conversion, and
/// match scrutinees are elaborated with `infer`, which rejects arithmetic —
/// so `match 1 + 1 { ... }` cannot appear in a type at all.
///
/// Once #48 (plus scrutinee inference) is fixed, `i` should elaborate and
/// `1 + 1` should convert to `2`, selecting `u32`.
#[test]
fn issue_48_meta_arithmetic_in_type_rejected() {
    let src = "def i() -> (match 1 + 1 { 2 => u32, _ => u0 }) = 5;";
    let err = elaborate_error(src);
    assert!(
        err.contains("cannot infer type of a primitive operation"),
        "unexpected error: {err}"
    );
}

/// Issues #72/#110: signatures are elaborated with an empty globals table, so
/// a definition cannot be referenced as a type in another signature (and even
/// with visibility, conversion would not δ-unfold `sel(0)` to `u32`).
///
/// Once fixed, `j` should elaborate with `sel(0)` convertible to `u32`.
#[test]
fn issue_72_signature_cannot_reference_global_type() {
    let src = "
        def sel(b: u1) -> Type = match b { 0 => u32, 1 => u64 };
        def j() -> sel(0) = 5;
    ";
    let err = elaborate_error(src);
    assert!(
        err.contains("unbound variable `sel`"),
        "unexpected error: {err}"
    );
}
