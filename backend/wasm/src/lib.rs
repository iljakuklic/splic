use anyhow::Result;
use splic_compiler::core::Program;

/// Compile a staged object-level program to a WebAssembly binary.
#[expect(clippy::todo, reason = "Wasm codegen not yet implemented")]
pub fn compile_wasm(_program: &Program<'_, '_>) -> Result<Vec<u8>> {
    todo!("Wasm codegen")
}
