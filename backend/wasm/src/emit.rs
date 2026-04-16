use std::collections::HashMap;

use anyhow::{Result, anyhow};
use splic_compiler::core::{Arm, IntWidth, Let, Match, Pat, Prim, Program, Term, de_bruijn::Ix};
use wasm_encoder::{BlockType, Instruction, ValType};

use super::types::{
    arith_mask, bitnot_mask, prim_result_valtype, term_to_valtype, width_to_valtype,
};

// ── FuncRegistry context ────────────────────────────────────────────────────────────

pub(crate) struct FuncRegistry<'names> {
    pub(crate) func_indices: HashMap<&'names str, u32>,
    func_return_types: HashMap<&'names str, ValType>,
}

impl<'names> FuncRegistry<'names> {
    pub(crate) fn from_program(program: &Program<'names, '_>) -> Result<Self> {
        let mut func_indices = HashMap::new();
        let mut func_return_types = HashMap::new();
        for (i, f) in program.defs.iter().enumerate() {
            let name = f.name.as_str();
            let idx = u32::try_from(i).map_err(|_| anyhow!("too many functions (> u32::MAX)"))?;
            func_indices.insert(name, idx);
            // Staged defs are all object-level functions (Term::Pi).
            let pi = match f.ty {
                splic_compiler::core::Term::Pi(pi) => pi,
                _ => unreachable!("staged def is not a function (unstager invariant)"),
            };
            func_return_types.insert(name, term_to_valtype(pi.body_ty));
        }
        Ok(Self {
            func_indices,
            func_return_types,
        })
    }
}

// ── Per-function emitter ───────────────────────────────────────────────────────

pub(crate) struct Emitter<'names, 'cg> {
    cg: &'cg FuncRegistry<'names>,
    /// Stack of `(local_index, ValType)` pairs; `locals[len-1-ix]` resolves `Var(Ix(ix))`.
    locals: Vec<(u32, ValType)>,
    /// Number of function parameters (params occupy local slots `0..n_params`).
    n_params: u32,
    /// Types of the extra (non-param) locals in allocation order.
    pub(crate) extra_local_types: Vec<ValType>,
    /// Accumulated Wasm instructions.
    pub(crate) instructions: Vec<Instruction<'static>>,
}

impl<'names, 'cg> Emitter<'names, 'cg> {
    pub(crate) fn new(cg: &'cg FuncRegistry<'names>, param_types: &[ValType]) -> Result<Self> {
        let n = u32::try_from(param_types.len())
            .map_err(|_| anyhow!("too many function parameters (> u32::MAX)"))?;
        Ok(Self {
            cg,
            locals: (0..n).zip(param_types.iter().copied()).collect(),
            n_params: n,
            extra_local_types: Vec::new(),
            instructions: Vec::new(),
        })
    }

    fn alloc_local(&mut self, vt: ValType) -> u32 {
        let idx = self.n_params
            + u32::try_from(self.extra_local_types.len()).expect("too many locals (> u32::MAX)");
        self.extra_local_types.push(vt);
        idx
    }

    fn push_binding(&mut self, idx: u32, vt: ValType) {
        self.locals.push((idx, vt));
    }

    fn pop_binding(&mut self) {
        self.locals.pop();
    }

    #[expect(
        clippy::indexing_slicing,
        reason = "pos is in bounds by De Bruijn invariant"
    )]
    fn resolve_var(&self, ix: Ix) -> (u32, ValType) {
        let pos = self.locals.len() - 1 - ix.as_usize();
        self.locals[pos]
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
                    .expect("unknown function in infer_valtype"),
                _ => unreachable!("unexpected function form in App"),
            },
            Term::Let(Let { body, .. }) => self.infer_valtype(body),
            Term::Match(Match { arms, .. }) => arms.first().map_or_else(
                || unreachable!("empty match"),
                |a| self.infer_valtype(a.body),
            ),
            _ => unreachable!("unexpected term in infer_valtype: {term:?}"),
        }
    }

    pub(crate) fn push(&mut self, instr: Instruction<'static>) {
        self.instructions.push(instr);
    }

    pub(crate) fn emit_term(&mut self, term: &Term<'names, '_>) {
        match term {
            Term::Lit(n, ty) => match ty.width {
                IntWidth::U0 => self.push(Instruction::I32Const(0)),
                // cast_signed() reinterprets the u64 bit pattern as i64. Wasm's i64.const
                // encodes the value as signed LEB128, but stores the same bit pattern, so
                // values >= 2^63 (which become negative i64) round-trip correctly.
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
                    .expect("undefined global");
                self.push(Instruction::Call(idx));
            }

            Term::Prim(_) => unreachable!("unapplied primitive in object-level term"),

            Term::App(app) => match app.func {
                Term::Prim(prim) => self.emit_prim_app(*prim, app.args),
                Term::Global(name) => {
                    for arg in app.args {
                        self.emit_term(arg);
                    }
                    let idx = *self
                        .cg
                        .func_indices
                        .get(name.as_str())
                        .expect("undefined global");
                    self.push(Instruction::Call(idx));
                }
                other => unreachable!("unsupported function in App: {other:?}"),
            },

            Term::Let(Let { expr, body, .. }) => {
                let vt = self.infer_valtype(expr);
                self.emit_term(expr);
                let local_idx = self.alloc_local(vt);
                self.push(Instruction::LocalSet(local_idx));
                self.push_binding(local_idx, vt);
                self.emit_term(body);
                self.pop_binding();
            }

            Term::Match(Match { scrutinee, arms }) => {
                self.emit_match(scrutinee, arms);
            }

            Term::Pi(_) | Term::Lam(_) | Term::Lift(_) | Term::Quote(_) | Term::Splice(_) => {
                unreachable!("unexpected term node in object-level code: {term:?}");
            }
        }
    }

    fn emit_prim_app(&mut self, prim: Prim, args: &[&Term<'names, '_>]) {
        match prim {
            Prim::Add(ty) => {
                self.emit_binop(args, Instruction::I32Add, Instruction::I64Add, ty.width);
                self.emit_arith_mask(ty.width);
            }
            Prim::Sub(ty) => {
                self.emit_binop(args, Instruction::I32Sub, Instruction::I64Sub, ty.width);
                self.emit_arith_mask(ty.width);
            }
            Prim::Mul(ty) => {
                self.emit_binop(args, Instruction::I32Mul, Instruction::I64Mul, ty.width);
                self.emit_arith_mask(ty.width);
            }
            Prim::Div(ty) => {
                self.emit_binop(args, Instruction::I32DivU, Instruction::I64DivU, ty.width);
                // div result is always in range; no mask needed
            }
            Prim::BitAnd(ty) => {
                self.emit_binop(args, Instruction::I32And, Instruction::I64And, ty.width);
            }
            Prim::BitOr(ty) => {
                self.emit_binop(args, Instruction::I32Or, Instruction::I64Or, ty.width);
            }
            Prim::BitNot(ty) => {
                let [arg] = args else {
                    unreachable!("BitNot requires exactly 1 argument")
                };
                self.emit_term(arg);
                if ty.width == IntWidth::U64 {
                    self.push(Instruction::I64Const(-1_i64));
                    self.push(Instruction::I64Xor);
                } else {
                    self.push(Instruction::I32Const(bitnot_mask(ty.width)));
                    self.push(Instruction::I32Xor);
                }
            }

            Prim::Eq(ty) => {
                self.emit_binop(args, Instruction::I32Eq, Instruction::I64Eq, ty.width);
            }
            Prim::Ne(ty) => {
                self.emit_binop(args, Instruction::I32Ne, Instruction::I64Ne, ty.width);
            }
            Prim::Lt(ty) => {
                self.emit_binop(args, Instruction::I32LtU, Instruction::I64LtU, ty.width);
            }
            Prim::Gt(ty) => {
                self.emit_binop(args, Instruction::I32GtU, Instruction::I64GtU, ty.width);
            }
            Prim::Le(ty) => {
                self.emit_binop(args, Instruction::I32LeU, Instruction::I64LeU, ty.width);
            }
            Prim::Ge(ty) => {
                self.emit_binop(args, Instruction::I32GeU, Instruction::I64GeU, ty.width);
            }

            Prim::IntTy(_) | Prim::U(_) | Prim::Embed(_) => {
                unreachable!("type-level or meta-only primitive in object-level term: {prim:?}");
            }
        }
    }

    fn emit_binop(
        &mut self,
        args: &[&Term<'names, '_>],
        i32_instr: Instruction<'static>,
        i64_instr: Instruction<'static>,
        width: IntWidth,
    ) {
        let [lhs, rhs] = args else {
            unreachable!("binary primitive requires exactly 2 arguments")
        };
        self.emit_term(lhs);
        self.emit_term(rhs);
        if width == IntWidth::U64 {
            self.push(i64_instr);
        } else {
            self.push(i32_instr);
        }
    }

    fn emit_arith_mask(&mut self, width: IntWidth) {
        if let Some(mask) = arith_mask(width) {
            self.push(Instruction::I32Const(mask));
            self.push(Instruction::I32And);
        }
    }

    fn emit_match(&mut self, scrutinee: &Term<'names, '_>, arms: &[Arm<'names, '_>]) {
        let scrutinee_vt = self.infer_valtype(scrutinee);
        let result_vt = arms.last().map_or_else(
            || unreachable!("empty match"),
            |a| self.infer_valtype(a.body),
        );

        // Store scrutinee in a temp local so we can test it repeatedly.
        let tmp = self.alloc_local(scrutinee_vt);
        self.emit_term(scrutinee);
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
                            // cast_signed() is correct here: i64.ne compares bit patterns,
                            // so values >= 2^63 (stored as negative i64) compare as equal
                            // to the corresponding u64 literal.
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
                    self.emit_term(arm.body);
                    self.push(Instruction::Br(1)); // exit outer result block
                    self.push(Instruction::End); // end guard block
                }
                Pat::Bind(_name) => {
                    // Bind scrutinee to a new local for the arm body.
                    let bind = self.alloc_local(scrutinee_vt);
                    self.push(Instruction::LocalGet(tmp));
                    self.push(Instruction::LocalSet(bind));
                    self.push_binding(bind, scrutinee_vt);
                    self.emit_term(arm.body);
                    self.pop_binding();
                }
                Pat::Wildcard => {
                    self.emit_term(arm.body);
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
    }
}
