mod emit;
mod types;

use anyhow::{Result, anyhow};
use splic_compiler::core::Program;
use wasm_encoder::{
    CodeSection, ExportKind, ExportSection, Function as WasmFunction, FunctionSection, Instruction,
    Module, TypeSection,
};

use emit::{Emitter, FuncRegistry};
use types::term_to_valtype;

/// Compile a staged object-level program to a WebAssembly binary.
pub fn compile_wasm(program: &Program<'_, '_>) -> Result<Vec<u8>> {
    let cg = FuncRegistry::from_program(program)?;

    let mut types = TypeSection::new();
    let mut functions = FunctionSection::new();
    let mut exports = ExportSection::new();
    let mut code = CodeSection::new();

    for (func_idx, func) in program.functions.iter().enumerate() {
        // Extract Wasm signature from the Pi type.
        let param_valtypes: Vec<_> = func
            .ty
            .params
            .iter()
            .map(|(_, ty)| term_to_valtype(ty))
            .collect();
        let result_valtype = term_to_valtype(func.ty.body_ty);

        let type_idx =
            u32::try_from(func_idx).map_err(|_| anyhow!("too many functions (> u32::MAX)"))?;
        types
            .ty()
            .function(param_valtypes.iter().copied(), [result_valtype]);
        functions.function(type_idx);
        exports.export(func.name.as_str(), ExportKind::Func, type_idx);

        // Emit the function body.
        let mut emitter = Emitter::new(&cg, &param_valtypes)?;
        emitter.emit_term(func.body);
        emitter.push(Instruction::End);

        // Declare extra locals (let-binding and scrutinee temporaries).
        let extra_locals: Vec<(u32, _)> = emitter
            .extra_local_types
            .iter()
            .map(|&vt| (1_u32, vt))
            .collect();
        let mut wasm_func = WasmFunction::new(extra_locals);
        for instr in &emitter.instructions {
            wasm_func.instruction(instr);
        }
        code.function(&wasm_func);
    }

    let mut module = Module::new();
    module.section(&types);
    module.section(&functions);
    module.section(&exports);
    module.section(&code);
    let bytes = module.finish();

    #[cfg(test)]
    wasmparser::validate(&bytes).expect("emitted invalid wasm module");

    Ok(bytes)
}
