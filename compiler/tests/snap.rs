use bumpalo::Bump;
use expect_test::expect_file;
use rstest::rstest;
use splic_compiler::{
    checker::elaborate_program, lexer::Lexer, parser::Parser, staging::unstage_program,
};
use std::path::PathBuf;

/// Run the full compiler pipeline on the input, writing a snapshot file for each phase.
///
/// Snapshot files are named `1_lex.txt`, `2_parse.txt`, `3_check.txt`, `6_stage.txt`
/// (slots 4–5 are reserved for future passes). On success the file contains the phase
/// output directly. On failure it begins with `ERROR` on the first line followed by
/// the error message (full context chain).
///
/// Later phases are skipped if an earlier phase fails.
#[rstest]
#[timeout(std::time::Duration::from_secs(if cfg!(miri) { 600 } else { 5 }))]
fn snap(#[files("tests/snap/*/*/0_input.splic")] path: PathBuf) {
    let dir = path.parent().unwrap();
    let input = std::fs::read_to_string(&path).unwrap();
    let arena = Bump::new();

    // ── Phase 1: Lex ────────────────────────────────────────────────────────
    let lex_result: Result<Vec<_>, _> = Lexer::new(&input).collect();
    let lex_snap = match &lex_result {
        Ok(tokens) => {
            use std::fmt::Write as _;
            tokens.iter().fold(String::new(), |mut s, t| {
                writeln!(s, "{t:?}").unwrap();
                s
            })
        }
        Err(e) => format!("ERROR\n{e:#}\n"),
    };
    expect_file![dir.join("1_lex.txt")].assert_eq(&lex_snap);
    let Ok(tokens) = lex_result else { return };

    // ── Phase 2: Parse ───────────────────────────────────────────────────────
    let parse_result = Parser::new(tokens.into_iter().map(Ok), &arena).parse_program();
    let parse_snap = match &parse_result {
        Ok(program) => format!("{program:#?}\n"),
        Err(e) => format!("ERROR\n{e:#}\n"),
    };
    expect_file![dir.join("2_parse.txt")].assert_eq(&parse_snap);
    let Ok(program) = parse_result else { return };

    // ── Phase 3: Check ───────────────────────────────────────────────────────
    let check_result = elaborate_program(&arena, &program);
    let check_snap = match &check_result {
        Ok(core) => format!("{core}\n"),
        Err(e) => format!("ERROR\n{e:#}\n"),
    };
    expect_file![dir.join("3_check.txt")].assert_eq(&check_snap);
    let Ok(core_program) = check_result else {
        return;
    };

    // ── Phase 6: Stage ───────────────────────────────────────────────────────
    // (slots 4–5 reserved for future optimisation passes)
    let stage_result = unstage_program(&arena, &core_program);
    let stage_snap = match &stage_result {
        Ok(staged) => format!("{staged}\n"),
        Err(e) => format!("ERROR\n{e:#}\n"),
    };
    expect_file![dir.join("6_stage.txt")].assert_eq(&stage_snap);
}
