use std::collections::HashMap;

use anyhow::{Result, anyhow, bail};
use splic_compiler::core::{Arm, IntWidth, Let, Match, Pat, Prim, Program, Term, de_bruijn::Ix};
use wasm_encoder::{
    BlockType, CodeSection, ExportKind, ExportSection, Function as WasmFunction, FunctionSection,
    Instruction, Module, TypeSection, ValType,
};

// ── Type helpers ───────────────────────────────────────────────────────────────

const fn width_to_valtype(width: IntWidth) -> ValType {
    match width {
        IntWidth::U64 => ValType::I64,
        _ => ValType::I32,
    }
}

/// Return the mask to apply after sub-word arithmetic, or `None` for word-size types.
const fn arith_mask(width: IntWidth) -> Option<i32> {
    match width {
        IntWidth::U1 => Some(0x1),
        IntWidth::U8 => Some(0xFF),
        IntWidth::U16 => Some(0xFFFF),
        _ => None,
    }
}

/// Return the XOR mask for `BitNot` on a non-U0, non-U64 integer width.
///
/// Precondition: `width` is one of `U1`, `U8`, `U16`, `U32`.
fn bitnot_mask(width: IntWidth) -> i32 {
    match width {
        IntWidth::U1 => 0x1,
        IntWidth::U8 => 0xFF,
        IntWidth::U16 => 0xFFFF,
        IntWidth::U32 => -1_i32, // 0xFFFF_FFFF
        IntWidth::U0 | IntWidth::U64 => {
            unreachable!("U0 and U64 are handled separately in emit_prim_app")
        }
    }
}

/// Extract the `ValType` from a `Term` that must be an integer type literal.
fn term_to_valtype(term: &Term<'_, '_>) -> Result<ValType> {
    match term {
        Term::Prim(Prim::IntTy(ty)) => Ok(width_to_valtype(ty.width)),
        other => bail!("expected integer type term, got {other:?}"),
    }
}

// ── Codegen context ────────────────────────────────────────────────────────────

struct Codegen<'names> {
    func_indices: HashMap<&'names str, u32>,
    func_return_types: HashMap<&'names str, ValType>,
}

impl<'names> Codegen<'names> {
    fn from_program(program: &Program<'names, '_>) -> Result<Self> {
        let mut func_indices = HashMap::new();
        let mut func_return_types = HashMap::new();
        for (i, f) in program.functions.iter().enumerate() {
            let name = f.name.as_str();
            let idx = u32::try_from(i).map_err(|_| anyhow!("too many functions (> u32::MAX)"))?;
            func_indices.insert(name, idx);
            func_return_types.insert(name, term_to_valtype(f.ty.body_ty)?);
        }
        Ok(Self {
            func_indices,
            func_return_types,
        })
    }
}

// ── Per-function emitter ───────────────────────────────────────────────────────

struct Emitter<'names, 'cg> {
    cg: &'cg Codegen<'names>,
    /// Stack of local indices; `local_stack[len-1-ix]` resolves `Var(Ix(ix))`.
    local_stack: Vec<u32>,
    /// `ValType` parallel to `local_stack`, used for type inference.
    local_types: Vec<ValType>,
    /// Next local slot to allocate (params occupy `0..n_params`).
    next_local: u32,
    /// Types of the extra (non-param) locals in allocation order.
    extra_local_types: Vec<ValType>,
    /// Accumulated Wasm instructions.
    instructions: Vec<Instruction<'static>>,
}

impl<'names, 'cg> Emitter<'names, 'cg> {
    fn new(cg: &'cg Codegen<'names>, param_types: &[ValType]) -> Result<Self> {
        let n = u32::try_from(param_types.len())
            .map_err(|_| anyhow!("too many function parameters (> u32::MAX)"))?;
        Ok(Self {
            cg,
            local_stack: (0..n).collect(),
            local_types: param_types.to_vec(),
            next_local: n,
            extra_local_types: Vec::new(),
            instructions: Vec::new(),
        })
    }

    fn alloc_local(&mut self, vt: ValType) -> u32 {
        let idx = self.next_local;
        self.next_local += 1;
        self.extra_local_types.push(vt);
        idx
    }

    fn push_binding(&mut self, idx: u32, vt: ValType) {
        self.local_stack.push(idx);
        self.local_types.push(vt);
    }

    fn pop_binding(&mut self) {
        self.local_stack.pop();
        self.local_types.pop();
    }

    #[expect(
        clippy::indexing_slicing,
        reason = "pos is in bounds by De Bruijn invariant"
    )]
    fn resolve_var(&self, ix: Ix) -> (u32, ValType) {
        let pos = self.local_stack.len() - 1 - ix.as_usize();
        (self.local_stack[pos], self.local_types[pos])
    }

    fn infer_valtype(&self, term: &Term<'names, '_>) -> ValType {
        match term {
            Term::Lit(_, ty) => width_to_valtype(ty.width),
            Term::Var(ix) => self.resolve_var(*ix).1,
            Term::App(app) => match app.func {
                Term::Prim(p) => prim_result_valtype(*p),
                Term::Global(name) => self
                    .cg
                    .func_return_types
                    .get(name.as_str())
                    .copied()
                    .unwrap_or(ValType::I32),
                _ => ValType::I32,
            },
            Term::Let(Let { body, .. }) => self.infer_valtype(body),
            Term::Match(Match { arms, .. }) => arms
                .first()
                .map_or(ValType::I32, |a| self.infer_valtype(a.body)),
            _ => ValType::I32,
        }
    }

    fn push(&mut self, instr: Instruction<'static>) {
        self.instructions.push(instr);
    }

    fn emit_term(&mut self, term: &Term<'names, '_>) -> Result<()> {
        match term {
            Term::Lit(n, ty) => match ty.width {
                IntWidth::U0 => self.push(Instruction::I32Const(0)),
                IntWidth::U64 => self.push(Instruction::I64Const((*n).cast_signed())),
                #[expect(
                    clippy::cast_possible_truncation,
                    reason = "smaller widths always fit in i32"
                )]
                _ => self.push(Instruction::I32Const(*n as i32)),
            },

            Term::Var(ix) => {
                let (local_idx, _vt) = self.resolve_var(*ix);
                self.push(Instruction::LocalGet(local_idx));
            }

            Term::Global(name) => {
                let idx = *self
                    .cg
                    .func_indices
                    .get(name.as_str())
                    .ok_or_else(|| anyhow!("undefined global: {name}"))?;
                self.push(Instruction::Call(idx));
            }

            Term::Prim(_) => bail!("unapplied primitive in object-level term"),

            Term::App(app) => match app.func {
                Term::Prim(prim) => self.emit_prim_app(*prim, app.args)?,
                Term::Global(name) => {
                    for arg in app.args {
                        self.emit_term(arg)?;
                    }
                    let idx = *self
                        .cg
                        .func_indices
                        .get(name.as_str())
                        .ok_or_else(|| anyhow!("undefined global: {name}"))?;
                    self.push(Instruction::Call(idx));
                }
                other => bail!("unsupported function in App: {other:?}"),
            },

            Term::Let(Let { expr, body, .. }) => {
                let vt = self.infer_valtype(expr);
                self.emit_term(expr)?;
                let local_idx = self.alloc_local(vt);
                self.push(Instruction::LocalSet(local_idx));
                self.push_binding(local_idx, vt);
                self.emit_term(body)?;
                self.pop_binding();
            }

            Term::Match(Match { scrutinee, arms }) => {
                self.emit_match(scrutinee, arms)?;
            }

            Term::Pi(_) | Term::Lam(_) | Term::Lift(_) | Term::Quote(_) | Term::Splice(_) => {
                bail!("unexpected term node in object-level code: {term:?}");
            }
        }
        Ok(())
    }

    fn emit_prim_app(&mut self, prim: Prim, args: &[&Term<'names, '_>]) -> Result<()> {
        // Emit two args and then a 32-bit or 64-bit instruction depending on width.
        macro_rules! binop {
            ($args:expr, $i32:expr, $i64:expr, $width:expr) => {{
                let [lhs, rhs] = $args else {
                    bail!("binary primitive requires exactly 2 arguments")
                };
                self.emit_term(lhs)?;
                self.emit_term(rhs)?;
                if $width == IntWidth::U64 {
                    self.push($i64);
                } else {
                    self.push($i32);
                }
            }};
        }

        match prim {
            // U0 erasure: any operation on u0 always produces 0.
            Prim::Add(ty)
            | Prim::Sub(ty)
            | Prim::Mul(ty)
            | Prim::Div(ty)
            | Prim::BitAnd(ty)
            | Prim::BitOr(ty)
            | Prim::BitNot(ty)
                if ty.width == IntWidth::U0 =>
            {
                self.push(Instruction::I32Const(0));
            }

            Prim::Add(ty) => {
                binop!(args, Instruction::I32Add, Instruction::I64Add, ty.width);
                self.emit_arith_mask(ty.width);
            }
            Prim::Sub(ty) => {
                binop!(args, Instruction::I32Sub, Instruction::I64Sub, ty.width);
                self.emit_arith_mask(ty.width);
            }
            Prim::Mul(ty) => {
                binop!(args, Instruction::I32Mul, Instruction::I64Mul, ty.width);
                self.emit_arith_mask(ty.width);
            }
            Prim::Div(ty) => {
                binop!(args, Instruction::I32DivU, Instruction::I64DivU, ty.width);
                // div result is always in range; no mask needed
            }
            Prim::BitAnd(ty) => {
                binop!(args, Instruction::I32And, Instruction::I64And, ty.width);
            }
            Prim::BitOr(ty) => {
                binop!(args, Instruction::I32Or, Instruction::I64Or, ty.width);
            }
            Prim::BitNot(ty) => {
                let [arg] = args else {
                    bail!("BitNot requires exactly 1 argument")
                };
                self.emit_term(arg)?;
                if ty.width == IntWidth::U64 {
                    self.push(Instruction::I64Const(-1_i64));
                    self.push(Instruction::I64Xor);
                } else {
                    self.push(Instruction::I32Const(bitnot_mask(ty.width)));
                    self.push(Instruction::I32Xor);
                }
            }

            Prim::Eq(ty) => {
                binop!(args, Instruction::I32Eq, Instruction::I64Eq, ty.width);
            }
            Prim::Ne(ty) => {
                binop!(args, Instruction::I32Ne, Instruction::I64Ne, ty.width);
            }
            Prim::Lt(ty) => {
                binop!(args, Instruction::I32LtU, Instruction::I64LtU, ty.width);
            }
            Prim::Gt(ty) => {
                binop!(args, Instruction::I32GtU, Instruction::I64GtU, ty.width);
            }
            Prim::Le(ty) => {
                binop!(args, Instruction::I32LeU, Instruction::I64LeU, ty.width);
            }
            Prim::Ge(ty) => {
                binop!(args, Instruction::I32GeU, Instruction::I64GeU, ty.width);
            }

            Prim::IntTy(_) | Prim::U(_) | Prim::Embed(_) => {
                bail!("type-level or meta-only primitive in object-level term: {prim:?}");
            }
        }
        Ok(())
    }

    fn emit_arith_mask(&mut self, width: IntWidth) {
        if let Some(mask) = arith_mask(width) {
            self.push(Instruction::I32Const(mask));
            self.push(Instruction::I32And);
        }
    }

    fn emit_match(&mut self, scrutinee: &Term<'names, '_>, arms: &[Arm<'names, '_>]) -> Result<()> {
        let scrutinee_vt = self.infer_valtype(scrutinee);
        let result_vt = arms
            .last()
            .map_or(ValType::I32, |a| self.infer_valtype(a.body));

        // Store scrutinee in a temp local so we can test it repeatedly.
        let tmp = self.alloc_local(scrutinee_vt);
        self.emit_term(scrutinee)?;
        self.push(Instruction::LocalSet(tmp));

        // Outer block that all literal arms break out of on a successful match.
        self.push(Instruction::Block(BlockType::Result(result_vt)));

        for arm in arms {
            match &arm.pat {
                Pat::Lit(n) => {
                    // Guard block: BrIf(0) skips to its end if pattern doesn't match,
                    // trying the next arm. Br(1) exits the outer result block on match.
                    self.push(Instruction::Block(BlockType::Empty));
                    self.push(Instruction::LocalGet(tmp));
                    match scrutinee_vt {
                        ValType::I64 => {
                            self.push(Instruction::I64Const((*n).cast_signed()));
                            self.push(Instruction::I64Ne);
                        }
                        _ => {
                            #[expect(
                                clippy::cast_possible_truncation,
                                reason = "literal patterns on sub-u64 types always fit in i32"
                            )]
                            self.push(Instruction::I32Const(*n as i32));
                            self.push(Instruction::I32Ne);
                        }
                    }
                    self.push(Instruction::BrIf(0)); // not this arm → exit guard block
                    self.emit_term(arm.body)?;
                    self.push(Instruction::Br(1)); // exit outer result block
                    self.push(Instruction::End); // end guard block
                }
                Pat::Bind(_name) => {
                    // Bind scrutinee to a new local for the arm body.
                    let bind = self.alloc_local(scrutinee_vt);
                    self.push(Instruction::LocalGet(tmp));
                    self.push(Instruction::LocalSet(bind));
                    self.push_binding(bind, scrutinee_vt);
                    self.emit_term(arm.body)?;
                    self.pop_binding();
                }
                Pat::Wildcard => {
                    self.emit_term(arm.body)?;
                }
            }
        }

        // If every arm is a literal pattern the match is exhaustive; the runtime can
        // never reach past the last guard block, but Wasm requires the block to be
        // well-typed, so emit `unreachable`.
        let has_catch_all = arms
            .iter()
            .any(|a| matches!(a.pat, Pat::Bind(_) | Pat::Wildcard));
        if !has_catch_all {
            self.push(Instruction::Unreachable);
        }

        self.push(Instruction::End); // end outer result block
        Ok(())
    }
}

// ── Result type of a primitive application ────────────────────────────────────

const fn prim_result_valtype(prim: Prim) -> ValType {
    match prim {
        Prim::Add(ty)
        | Prim::Sub(ty)
        | Prim::Mul(ty)
        | Prim::Div(ty)
        | Prim::BitAnd(ty)
        | Prim::BitOr(ty)
        | Prim::BitNot(ty) => width_to_valtype(ty.width),
        // Comparisons always produce a u1 (i32 0 or 1).
        Prim::Eq(_) | Prim::Ne(_) | Prim::Lt(_) | Prim::Gt(_) | Prim::Le(_) | Prim::Ge(_) => {
            ValType::I32
        }
        Prim::IntTy(_) | Prim::U(_) | Prim::Embed(_) => ValType::I32,
    }
}

// ── Public entry point ─────────────────────────────────────────────────────────

/// Compile a staged object-level program to a WebAssembly binary.
pub fn compile_wasm(program: &Program<'_, '_>) -> Result<Vec<u8>> {
    let cg = Codegen::from_program(program)?;

    let mut types = TypeSection::new();
    let mut functions = FunctionSection::new();
    let mut exports = ExportSection::new();
    let mut code = CodeSection::new();

    for (func_idx, func) in program.functions.iter().enumerate() {
        // Extract Wasm signature from the Pi type.
        let param_valtypes: Vec<ValType> = func
            .ty
            .params
            .iter()
            .map(|(_, ty)| term_to_valtype(ty))
            .collect::<Result<_>>()?;
        let result_valtype = term_to_valtype(func.ty.body_ty)?;

        let type_idx =
            u32::try_from(func_idx).map_err(|_| anyhow!("too many functions (> u32::MAX)"))?;
        types
            .ty()
            .function(param_valtypes.iter().copied(), [result_valtype]);
        functions.function(type_idx);
        exports.export(func.name.as_str(), ExportKind::Func, type_idx);

        // Emit the function body.
        let mut emitter = Emitter::new(&cg, &param_valtypes)?;
        emitter.emit_term(func.body)?;
        emitter.push(Instruction::End);

        // Declare extra locals (let-binding and scrutinee temporaries).
        let extra_locals: Vec<(u32, ValType)> = emitter
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
    Ok(module.finish())
}
