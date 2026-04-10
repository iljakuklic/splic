use splic_compiler::core::{IntWidth, Prim, Term};
use wasm_encoder::ValType;

/// Map a Splic integer width to the Wasm value type used to represent it.
pub(crate) const fn width_to_valtype(width: IntWidth) -> ValType {
    match width {
        IntWidth::U64 => ValType::I64,
        _ => ValType::I32,
    }
}

/// Return the mask to apply after sub-word arithmetic, or `None` for word-size types.
pub(crate) const fn arith_mask(width: IntWidth) -> Option<i32> {
    match width {
        IntWidth::U1 => Some(0x1),
        IntWidth::U8 => Some(0xFF),
        IntWidth::U16 => Some(0xFFFF),
        _ => None,
    }
}

/// Return the XOR mask for `BitNot` on a non-U64 integer width.
///
/// XOR-ing the operand with this mask flips exactly the meaningful bits, giving
/// the bitwise NOT within the type's width. U0 has no meaningful bits, so its
/// mask is 0 (leaving the always-zero value unchanged).
pub(crate) fn bitnot_mask(width: IntWidth) -> i32 {
    match width {
        IntWidth::U0 => 0,
        IntWidth::U1 => 0x1,
        IntWidth::U8 => 0xFF,
        IntWidth::U16 => 0xFFFF,
        IntWidth::U32 => -1_i32, // 0xFFFF_FFFF
        IntWidth::U64 => unreachable!("U64 BitNot uses i64 path"),
    }
}

/// Extract the `ValType` from a `Term` that must be an integer type literal.
pub(crate) fn term_to_valtype(term: &Term<'_, '_>) -> ValType {
    match term {
        Term::Prim(Prim::IntTy(ty)) => width_to_valtype(ty.width),
        other => unreachable!("expected integer type term, got {other:?}"),
    }
}

/// Return the Wasm result type produced by applying `prim`.
pub(crate) fn prim_result_valtype(prim: Prim) -> ValType {
    width_to_valtype(prim.result_width())
}
