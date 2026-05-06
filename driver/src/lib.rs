use anyhow::{Context as _, Result};
use bumpalo::Bump;
use splic_compiler::{checker, core, lexer, parser, staging};

/// Run all compiler phases up to and including staging.
///
/// The returned `Program` borrows from `arena` for both names and terms.
/// Use `format!("{program}")` to pretty-print the staged output.
pub fn stage<'arena>(source: &str, arena: &'arena Bump) -> Result<core::Program<'arena, 'arena>> {
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

/// Compilation target.
#[non_exhaustive]
#[derive(Clone, Copy)]
pub enum Target {
    /// WebAssembly binary format.
    Wasm,
}

/// Compile source to a target binary.
pub fn compile(source: &str, target: Target) -> Result<Vec<u8>> {
    let arena = Bump::new();
    let program = stage(source, &arena)?;
    match target {
        Target::Wasm => compile_wasm(&program),
    }
}

/// Compile a staged program to WebAssembly.
///
/// Returns an error if the `backend-wasm` feature is not enabled.
fn compile_wasm(program: &core::Program<'_, '_>) -> Result<Vec<u8>> {
    cfg_select! {
        feature = "backend-wasm" => {
            splic_backend_wasm::compile_wasm(program)
        }
        _ => {
            let _ = program;
            anyhow::bail!("Wasm backend not enabled; recompile with --features backend-wasm")
        }
    }
}
