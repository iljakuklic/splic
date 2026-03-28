use crate::core::{self, IntWidth, Phase};

pub use ctx::Ctx;
pub use elaborate::{collect_signatures, elaborate_program};
pub use infer::{check, check_val, infer};

mod ctx;
mod elaborate;
mod infer;

/// Resolve a built-in type name to a static core term, using `phase` for integer types.
///
/// Returns `None` if the name is not a built-in type.
pub(crate) fn builtin_prim_ty(
    name: &'_ core::Name,
    phase: Phase,
) -> Option<&'static core::Term<'static>> {
    Some(match name.as_str() {
        "u0" => core::Term::int_ty(IntWidth::U0, phase),
        "u1" => core::Term::int_ty(IntWidth::U1, phase),
        "u8" => core::Term::int_ty(IntWidth::U8, phase),
        "u16" => core::Term::int_ty(IntWidth::U16, phase),
        "u32" => core::Term::int_ty(IntWidth::U32, phase),
        "u64" => core::Term::int_ty(IntWidth::U64, phase),
        "Type" => &core::Term::TYPE,
        "VmType" => &core::Term::VM_TYPE,
        _ => return None,
    })
}

#[cfg(test)]
mod test;
