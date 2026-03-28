use crate::core::{self, IntType, IntWidth, Lvl, Phase, Prim, alpha_eq, value};

pub use ctx::Ctx;
pub use elaborate::{elaborate_program, collect_signatures};
pub use infer::{check, check_val, infer};

mod ctx;
mod elaborate;
mod infer;

/// Resolve a built-in type name to a static core term, using `phase` for integer types.
///
/// Returns `None` if the name is not a built-in type.
pub(crate) fn builtin_prim_ty(name: &'_ core::Name, phase: Phase) -> Option<&'static core::Term<'static>> {
    Some(match name.as_str() {
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

/// Return the universe phase that a Value type inhabits, or `None` if unknown.
///
/// This is the `NbE` analogue of the 2LTT kinding judgement.
const fn value_type_universe(ty: &value::Value<'_>) -> Option<Phase> {
    match ty {
        value::Value::Prim(Prim::IntTy(IntType { phase, .. })) => Some(*phase),
        value::Value::Prim(Prim::U(_)) | value::Value::Lift(_) | value::Value::Pi(_) => {
            Some(Phase::Meta)
        }
        // Neutral or unknown — can't determine phase
        value::Value::Rigid(_)
        | value::Value::Global(_)
        | value::Value::App(_, _)
        | value::Value::Prim(_)
        | value::Value::Lit(..)
        | value::Value::Lam(_)
        | value::Value::Quote(_) => None,
    }
}

/// Return the universe phase that a Value type inhabits, using context to look up
/// type variables. Returns `None` if phase is still indeterminate.
pub(crate) fn value_type_universe_ctx<'core>(ctx: &Ctx<'core, '_>, ty: &value::Value<'core>) -> Option<Phase> {
    match value_type_universe(ty) {
        Some(phase) => Some(phase),
        None => {
            // Look up the type of a variable
            if let value::Value::Rigid(lvl) = ty {
                let ix = lvl.ix_at_depth(ctx.lvl);
                ctx.types.get(ctx.types.len() - 1 - ix.0).and_then(|t| value_type_universe_ctx(ctx, t))
            } else {
                None
            }
        }
    }
}

/// Check if two values are equal as types (definitionally).
pub(crate) fn types_equal_val(
    arena: &bumpalo::Bump,
    depth: Lvl,
    a: &value::Value<'_>,
    b: &value::Value<'_>,
) -> bool {
    let ta = value::quote(arena, depth, a);
    let tb = value::quote(arena, depth, b);
    alpha_eq(ta, tb)
}

#[cfg(test)]
mod test;
