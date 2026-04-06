#![allow(clippy::unwrap_used, reason = "test code")]
#![expect(clippy::indexing_slicing, reason = "test code")]

use bumpalo::Bump;
use expect_test::expect_file;
use rstest::rstest;
use splic_compiler::{
    checker::elaborate_program, lexer::Lexer, parser::Parser, staging::unstage_program,
};
use std::path::{Path, PathBuf};

/// The compiler phases that produce snapshot files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Lex,
    Parse,
    Check,
    Stage,
}

impl Phase {
    const ALL: &'static [Self] = &[Self::Lex, Self::Parse, Self::Check, Self::Stage];

    const fn snap_filename(self) -> &'static str {
        match self {
            Self::Lex => "1_lex.txt",
            Self::Parse => "2_parse.txt",
            Self::Check => "3_check.txt",
            Self::Stage => "6_stage.txt",
        }
    }

    fn phases_after(self) -> &'static [Self] {
        let pos = Self::ALL.iter().position(|&p| p == self).unwrap();
        &Self::ALL[pos + 1..]
    }
}

/// The expected outcome of running the compiler pipeline on a test case.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExpectedOutcome {
    /// All phases run and succeed.
    Success,
    /// Phases run only up to (and including) the given phase, which succeeds.
    /// Later phases are intentionally skipped and must have no snapshot files on disk.
    StopAfter(Phase),
    /// Phases run until the given phase, which is expected to fail.
    /// Later phases are skipped and must have no snapshot files on disk.
    FailAt(Phase),
}

/// Maps a top-level snap folder name to its expected compiler outcome.
///
/// When adding a new folder under `tests/snap/`, add a corresponding entry here.
fn expected_outcome(folder: &str) -> ExpectedOutcome {
    match folder {
        "full" => ExpectedOutcome::Success,
        "lex" => ExpectedOutcome::StopAfter(Phase::Lex),
        "lex_error" => ExpectedOutcome::FailAt(Phase::Lex),
        "parse_error" => ExpectedOutcome::FailAt(Phase::Parse),
        "type_error" => ExpectedOutcome::FailAt(Phase::Check),
        "stage_error" => ExpectedOutcome::FailAt(Phase::Stage),
        other => panic!(
            "unknown test folder {other:?}; add it to `expected_outcome` in compiler/tests/snap.rs"
        ),
    }
}

/// Asserts that the actual pipeline outcome matches the expected one, and that no
/// snapshot files remain on disk for phases that should have been skipped.
fn assert_outcome(
    expected: ExpectedOutcome,
    failed_at: Option<Phase>,
    stopped_after: Phase,
    dir: &Path,
) {
    match (expected, failed_at) {
        (ExpectedOutcome::Success, None) => {}
        (ExpectedOutcome::StopAfter(exp), None) => {
            assert_eq!(
                exp,
                stopped_after,
                "expected to stop after {exp:?} but stopped after {stopped_after:?} in {}",
                dir.display(),
            );
        }
        (ExpectedOutcome::FailAt(exp), Some(actual)) => {
            assert_eq!(
                exp,
                actual,
                "expected failure at {exp:?} but got failure at {actual:?} in {}",
                dir.display(),
            );
        }
        _ => {
            let actual = match failed_at {
                None => format!("Success (last phase: {stopped_after:?})"),
                Some(p) => format!("FailAt({p:?})"),
            };
            panic!(
                "compiler outcome mismatch in {}: expected {expected:?} but got {actual}",
                dir.display()
            );
        }
    }

    // Ensure no stale snapshot files exist for skipped phases.
    let skipped_after = failed_at.unwrap_or(stopped_after);
    for &later in skipped_after.phases_after() {
        let snap_path = dir.join(later.snap_filename());
        assert!(
            !snap_path.exists(),
            "leftover snapshot {} must not exist after {skipped_after:?}",
            snap_path.display(),
        );
    }
}

/// Run the full compiler pipeline on the input, writing a snapshot file for each phase.
///
/// Snapshot files are named `1_lex.txt`, `2_parse.txt`, `3_check.txt`, `6_stage.txt`
/// (slots 4–5 are reserved for future passes). On success the file contains the phase
/// output directly. On failure it begins with `ERROR` on the first line followed by
/// the error message (full context chain).
///
/// The top-level folder name determines which phase (if any) is expected to fail and
/// which later snapshots must be absent; see `expected_outcome` for the mapping.
#[rstest]
#[timeout(std::time::Duration::from_secs(if cfg!(miri) { 600 } else { 5 }))]
fn snap(#[files("tests/snap/*/*/0_input.splic")] path: PathBuf) {
    let dir = path.parent().unwrap();
    let folder = dir.parent().unwrap().file_name().unwrap().to_str().unwrap();
    let expected = expected_outcome(folder);
    let input = std::fs::read_to_string(&path).unwrap();
    let arena = Bump::new();

    // ── Phase 1: Lex ────────────────────────────────────────────────────────
    let lex_result: Result<Vec<_>, _> = Lexer::new(&input, &arena).collect();
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
    match lex_result {
        Err(_) => {
            assert_outcome(expected, Some(Phase::Lex), Phase::Lex, dir);
            return;
        }
        Ok(_) if expected == ExpectedOutcome::StopAfter(Phase::Lex) => {
            assert_outcome(expected, None, Phase::Lex, dir);
            return;
        }
        Ok(tokens) => {
            // ── Phase 2: Parse ───────────────────────────────────────────────────────
            let parse_result = Parser::new(tokens.into_iter().map(Ok), &arena).parse_program();
            let parse_snap = match &parse_result {
                Ok(program) => format!("{program:#?}\n"),
                Err(e) => format!("ERROR\n{e:#}\n"),
            };
            expect_file![dir.join("2_parse.txt")].assert_eq(&parse_snap);
            match parse_result {
                Err(_) => {
                    assert_outcome(expected, Some(Phase::Parse), Phase::Parse, dir);
                    return;
                }
                Ok(_) if expected == ExpectedOutcome::StopAfter(Phase::Parse) => {
                    assert_outcome(expected, None, Phase::Parse, dir);
                    return;
                }
                Ok(program) => {
                    // ── Phase 3: Check ───────────────────────────────────────────────────
                    let check_result = elaborate_program(&arena, &program);
                    let check_snap = match &check_result {
                        Ok(core) => format!("{core}\n"),
                        Err(e) => format!("ERROR\n{e:#}\n"),
                    };
                    expect_file![dir.join("3_check.txt")].assert_eq(&check_snap);
                    match check_result {
                        Err(_) => {
                            assert_outcome(expected, Some(Phase::Check), Phase::Check, dir);
                            return;
                        }
                        Ok(_) if expected == ExpectedOutcome::StopAfter(Phase::Check) => {
                            assert_outcome(expected, None, Phase::Check, dir);
                            return;
                        }
                        Ok(core_program) => {
                            // ── Phase 6: Stage ───────────────────────────────────────────────
                            // (slots 4–5 reserved for future optimisation passes)
                            let stage_result = unstage_program(&arena, &core_program);
                            let stage_snap = match &stage_result {
                                Ok(staged) => format!("{staged}\n"),
                                Err(e) => format!("ERROR\n{e:#}\n"),
                            };
                            expect_file![dir.join("6_stage.txt")].assert_eq(&stage_snap);
                            let failed_at = stage_result.is_err().then_some(Phase::Stage);
                            assert_outcome(expected, failed_at, Phase::Stage, dir);
                        }
                    }
                }
            }
        }
    }
}
