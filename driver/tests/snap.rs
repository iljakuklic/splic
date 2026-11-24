#![allow(clippy::unwrap_used, reason = "test code")]
#![expect(clippy::indexing_slicing, reason = "test code")]

mod common;

use bumpalo::Bump;
use common::global_alpha_eq;
use expect_test::expect_file;
use rstest::rstest;
use splic_compiler::{
    checker::elaborate_program,
    core::{Global, Program, Term},
    lexer::Lexer,
    parser::Parser,
    staging::unstage_program,
};
use std::path::{Path, PathBuf};

/// The compiler phases that produce snapshot files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Lex,
    Parse,
    Check,
    Stage,
    Wasm,
}

impl Phase {
    const ALL: &'static [Self] = &[Self::Lex, Self::Parse, Self::Check, Self::Stage, Self::Wasm];

    const fn snap_filename(self) -> &'static str {
        match self {
            Self::Lex => "1_lex.txt",
            Self::Parse => "2_parse.txt",
            Self::Check => "3_check.txt",
            Self::Stage => "6_stage.txt",
            Self::Wasm => "8_wasm.wat",
        }
    }

    fn phases_after(self) -> &'static [Self] {
        let pos = Self::ALL.iter().position(|&p| p == self).unwrap();
        &Self::ALL[pos + 1..]
    }
}

/// The expected outcome of running the compiler pipeline on a test case.
///
/// `ok` and `last_phase` are orthogonal: `ok` says whether the last phase
/// should succeed, and `last_phase` says which phase is last (either because
/// the pipeline is intentionally stopped there, or because that phase fails).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ExpectedOutcome {
    ok: bool,
    last_phase: Phase,
}

impl ExpectedOutcome {
    const fn new(ok: bool, last_phase: Phase) -> Self {
        Self { ok, last_phase }
    }

    /// All phases run successfully up to and including `phase`.
    const fn run_till(phase: Phase) -> Self {
        Self::new(true, phase)
    }

    /// Phases run until `phase`, which is expected to fail.
    const fn fail_at(phase: Phase) -> Self {
        Self::new(false, phase)
    }
}

/// Maps a top-level snap folder name to its expected compiler outcome.
///
/// When adding a new folder under `tests/snap/`, add a corresponding entry here.
fn expected_outcome(folder: &str) -> ExpectedOutcome {
    match folder {
        "full" | "examples" => ExpectedOutcome::run_till(Phase::Wasm),
        "stage" => ExpectedOutcome::run_till(Phase::Stage),
        "lex" => ExpectedOutcome::run_till(Phase::Lex),
        "lex_error" => ExpectedOutcome::fail_at(Phase::Lex),
        "parse_error" => ExpectedOutcome::fail_at(Phase::Parse),
        "type_error" => ExpectedOutcome::fail_at(Phase::Check),
        "stage_error" => ExpectedOutcome::fail_at(Phase::Stage),
        other => panic!(
            "unknown test folder {other:?}; add it to `expected_outcome` in compiler/tests/snap.rs"
        ),
    }
}

/// Asserts that the actual pipeline outcome matches the expected one, and that no
/// snapshot files remain on disk for phases that should have been skipped.
fn assert_outcome(expected: ExpectedOutcome, ok: bool, last_phase: Phase, dir: &Path) {
    assert_eq!(
        ok,
        expected.ok,
        "phase {last_phase:?} succeeded={ok} but expected succeeded={} in {}",
        expected.ok,
        dir.display(),
    );
    assert_eq!(
        last_phase,
        expected.last_phase,
        "pipeline stopped at {last_phase:?} but expected to stop at {:?} in {}",
        expected.last_phase,
        dir.display(),
    );
    for &later in last_phase.phases_after() {
        let snap_path = dir.join(later.snap_filename());
        assert!(
            !snap_path.exists(),
            "leftover snapshot {} must not exist after {last_phase:?}",
            snap_path.display(),
        );
    }
}

/// Write a snapshot and decide whether to continue the pipeline.
///
/// Writes `snap` to the snapshot file for `phase`, then:
/// - If `result` is `Ok` and a `post` check is given: runs it on the inner value. It runs
///   after the snapshot is written, so a failing check still leaves the snapshot to inspect.
/// - If `result` is `Err`: calls `assert_outcome` (expecting failure) and returns.
/// - If `result` is `Ok` and this is `expected.last_phase`: calls `assert_outcome`
///   (expecting success) and returns.
/// - Otherwise: evaluates to the inner `Ok` value so the next phase can proceed.
macro_rules! phase {
    ($snap:expr, $result:expr, $phase:expr, $expected:expr, $dir:expr) => {
        phase!($snap, $result, $phase, $expected, $dir, |_| ())
    };
    ($snap:expr, $result:expr, $phase:expr, $expected:expr, $dir:expr, $post:expr) => {{
        expect_file![$dir.join($phase.snap_filename())].assert_eq(&$snap);
        if let Ok(val) = &$result {
            $post(val);
        }
        match $result {
            Err(_) => {
                assert_outcome($expected, false, $phase, $dir);
                return;
            }
            Ok(_) if $phase == $expected.last_phase => {
                assert_outcome($expected, true, $phase, $dir);
                return;
            }
            Ok(val) => val,
        }
    }};
}

/// Check the staging pass's correctness properties on a successfully staged program:
///
/// - **Stability**: a `code def` containing no splices stages to itself.
/// - **Idempotence**: staging the staged output again changes nothing (it is splice-free,
///   so this is stability applied to every program in the suite).
fn check_staging_properties(core: &Program<'_, '_>, staged: &Program<'_, '_>) {
    for def in core
        .defs
        .iter()
        .filter(|d| !global_contains_splice(&d.global))
    {
        if let Global::Meta(_) = def.global {
            continue;
        }
        let out = staged
            .defs
            .iter()
            .find(|d| d.name == def.name)
            .unwrap_or_else(|| panic!("code def `{}` missing from staged output", def.name));
        assert!(
            global_alpha_eq(&def.global, &out.global),
            "stability: splice-free code def changed by staging\n\
             before:\n{def}\nafter:\n{out}",
        );
    }

    let arena = Bump::new();
    let restaged = unstage_program(&arena, staged).expect("restaging staged output failed");
    assert_eq!(
        staged.defs.len(),
        restaged.defs.len(),
        "restaging changed def count"
    );
    for (a, b) in staged.defs.iter().zip(restaged.defs) {
        assert!(
            a.name == b.name && global_alpha_eq(&a.global, &b.global),
            "idempotence: restaging changed a def\nstaged:\n{a}\nrestaged:\n{b}",
        );
    }
}

fn global_contains_splice(global: &Global<'_, '_>) -> bool {
    match global {
        Global::Meta(m) => contains_splice(m.ty) || contains_splice(m.body),
        Global::CodeFn(f) => {
            f.params.iter().any(|(_, t)| contains_splice(t))
                || contains_splice(f.ret_ty)
                || contains_splice(f.body)
        }
        Global::CodeConst(c) => contains_splice(c.ty) || contains_splice(c.body),
    }
}

/// Whether `term` contains a splice anywhere. The match is exhaustive on purpose, so a
/// new `Term` variant must be handled here.
fn contains_splice(term: &Term<'_, '_>) -> bool {
    match term {
        Term::Splice(_) => true,
        Term::Var(_) | Term::Prim(_) | Term::Lit(..) | Term::Global(_) => false,
        Term::App(app) => contains_splice(app.func) || app.args.iter().any(|a| contains_splice(a)),
        Term::Pi(pi) => {
            pi.params.iter().any(|(_, t)| contains_splice(t)) || contains_splice(pi.body_ty)
        }
        Term::Lam(lam) => {
            lam.params.iter().any(|(_, t)| contains_splice(t)) || contains_splice(lam.body)
        }
        Term::Lift(t) | Term::Quote(t) => contains_splice(t),
        Term::Let(l) => contains_splice(l.ty) || contains_splice(l.expr) || contains_splice(l.body),
        Term::Match(m) => {
            contains_splice(m.scrutinee) || m.arms.iter().any(|arm| contains_splice(arm.body))
        }
    }
}

/// Run the full compiler pipeline on the input, writing a snapshot file for each phase.
///
/// Snapshot files are named `1_lex.txt`, `2_parse.txt`, `3_check.txt`, `6_stage.txt`,
/// `8_wasm.wat` (slots 4–5 and 7 are reserved for future passes). On success the file
/// contains the phase output directly. On failure it begins with `ERROR` on the first
/// line followed by the error message (full context chain).
///
/// The top-level folder name determines which phase (if any) is expected to fail and
/// which later snapshots must be absent; see `expected_outcome` for the mapping.
///
/// The Wasm phase (8) requires the `backend-wasm` feature. When the feature is absent
/// the step is skipped and the test exits at Stage without writing `8_wasm.wat`.
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
    let tokens = phase!(lex_snap, lex_result, Phase::Lex, expected, dir);

    // ── Phase 2: Parse ───────────────────────────────────────────────────────
    let parse_result = Parser::new(tokens.into_iter().map(Ok), &arena).parse_program();
    let parse_snap = match &parse_result {
        Ok(program) => format!("{program:#?}\n"),
        Err(e) => format!("ERROR\n{e:#}\n"),
    };
    let program = phase!(parse_snap, parse_result, Phase::Parse, expected, dir);

    // ── Phase 3: Check ───────────────────────────────────────────────────────
    let check_result = elaborate_program(&arena, &program);
    let check_snap = match &check_result {
        Ok(core) => format!("{core}\n"),
        Err(e) => format!("ERROR\n{e:#}\n"),
    };
    let core_program = phase!(check_snap, check_result, Phase::Check, expected, dir);

    // ── Phase 6: Stage ───────────────────────────────────────────────────────
    // (slots 4–5 reserved for future optimisation passes)
    let stage_result = unstage_program(&arena, &core_program);
    let stage_snap = match &stage_result {
        Ok(staged) => format!("{staged}\n"),
        Err(e) => format!("ERROR\n{e:#}\n"),
    };
    let staged = phase!(
        stage_snap,
        stage_result,
        Phase::Stage,
        expected,
        dir,
        |staged| check_staging_properties(&core_program, staged)
    );

    // ── Phase 8: Wasm ────────────────────────────────────────────────────────
    // (slot 7 reserved for a future optimisation pass)
    // When the backend-wasm feature is absent this step is a no-op: no snapshot
    // is written and the pipeline ends at Stage rather than Wasm.
    #[cfg(feature = "backend-wasm")]
    {
        let wasm_result = splic_backend_wasm::compile_wasm(&staged)
            .and_then(|bytes| wasmprinter::print_bytes(&bytes));
        let wasm_snap = match &wasm_result {
            Ok(wat) => wat.clone(),
            Err(e) => format!("ERROR\n{e:#}\n"),
        };
        phase!(wasm_snap, wasm_result, Phase::Wasm, expected, dir);
    }
    #[cfg(not(feature = "backend-wasm"))]
    let _ = staged;
}
