use anyhow::{Context as _, Result};
use bumpalo::Bump;
use splic_compiler::{checker, core, lexer, parser, staging};

/// Run all compiler phases up to and including staging.
/// Returns the staged program pretty-printed.
pub fn stage(source: &str) -> Result<String> {
    let arena = Bump::new();
    let program = run_pipeline(source, &arena)?;
    Ok(format!("{program}"))
}

/// Compilation target.
#[non_exhaustive]
#[derive(Clone, Copy)]
pub enum Target {
    /// WebAssembly binary format.
    #[cfg(feature = "wasm")]
    Wasm,
}

/// Compile source to a target binary.
#[cfg_attr(
    not(any(feature = "wasm")),
    expect(unused_variables, reason = "no backends are enabled")
)]
pub fn compile(source: &str, target: Target) -> Result<Vec<u8>> {
    let arena = Bump::new();
    let program = run_pipeline(source, &arena)?;
    match target {
        #[cfg(feature = "wasm")]
        Target::Wasm => splic_backend_wasm::compile_wasm(&program),
    }
}

/// Run lex → parse → elaborate → unstage, allocating into `arena`.
///
/// The returned `Program` borrows from `arena` for both names and terms.
fn run_pipeline<'arena>(
    source: &str,
    arena: &'arena Bump,
) -> Result<core::Program<'arena, 'arena>> {
    let ast_arena = Bump::new();

    let lexer = lexer::Lexer::new(source, arena);
    let mut parser = parser::Parser::new(lexer, &ast_arena);
    let ast = parser.parse_program().context("failed to parse program")?;

    let core_arena = Bump::new();
    let core_program =
        checker::elaborate_program(&core_arena, &ast).context("failed to elaborate program")?;
    drop(ast_arena);

    let staged =
        staging::unstage_program(arena, &core_program).context("failed to stage program")?;
    drop(core_arena);

    Ok(staged)
}
