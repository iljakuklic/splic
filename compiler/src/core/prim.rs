use crate::common::Phase;

/// Integer widths for primitive types and operations
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IntWidth {
    U0,
    U1,
    U8,
    U16,
    U32,
    U64,
}

impl IntWidth {
    /// Returns the source-level name of this integer width (e.g. `"u64"`).
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::U0 => "u0",
            Self::U1 => "u1",
            Self::U8 => "u8",
            Self::U16 => "u16",
            Self::U32 => "u32",
            Self::U64 => "u64",
        }
    }
}

impl std::fmt::Display for IntWidth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.as_str().fmt(f)
    }
}

/// Integer type: width + phase (meta vs. object)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct IntType {
    pub width: IntWidth,
    pub phase: Phase,
}

impl IntType {
    pub const fn new(width: IntWidth, phase: Phase) -> Self {
        Self { width, phase }
    }
}

/// Built-in types and operations, fully resolved by the elaborator
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Prim {
    // Integer type (inhabits VmType at object phase, Type at meta phase)
    IntTy(IntType),
    // Universe: U(Meta) = Type, U(Object) = VmType
    U(Phase),
    // Arithmetic (binary)
    Add(IntType),
    Sub(IntType),
    Mul(IntType),
    Div(IntType),
    // Bitwise
    BitAnd(IntType),
    BitOr(IntType),
    BitNot(IntType),
    // Embed a meta-level integer into object-level code: IntTy(w, Meta) -> [[IntTy(w, Object)]]
    Embed(IntWidth),
    // Comparison (return U1 at the same phase)
    Eq(IntType),
    Ne(IntType),
    Lt(IntType),
    Gt(IntType),
    Le(IntType),
    Ge(IntType),
}

impl Prim {
    /// Returns `true` if this primitive is a binary infix operator.
    pub const fn is_binop(self) -> bool {
        matches!(
            self,
            Self::Add(_)
                | Self::Sub(_)
                | Self::Mul(_)
                | Self::Div(_)
                | Self::BitAnd(_)
                | Self::BitOr(_)
                | Self::Eq(_)
                | Self::Ne(_)
                | Self::Lt(_)
                | Self::Gt(_)
                | Self::Le(_)
                | Self::Ge(_)
        )
    }

    /// Returns the source-level symbol string for this primitive.
    pub const fn symbol_str(self) -> &'static str {
        match self {
            Self::IntTy(it) => it.width.as_str(),
            Self::U(Phase::Meta) => "Type",
            Self::U(Phase::Object) => "VmType",
            Self::Add(_) => "+",
            Self::Sub(_) => "-",
            Self::Mul(_) => "*",
            Self::Div(_) => "/",
            Self::BitAnd(_) => "&",
            Self::BitOr(_) => "|",
            Self::BitNot(_) => "!",
            Self::Embed(_) => "__embed__",
            Self::Eq(_) => "==",
            Self::Ne(_) => "!=",
            Self::Lt(_) => "<",
            Self::Gt(_) => ">",
            Self::Le(_) => "<=",
            Self::Ge(_) => ">=",
        }
    }
}
