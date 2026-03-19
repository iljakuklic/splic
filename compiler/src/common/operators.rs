//! Operator types and precedence/associativity rules.
//!
//! These types represent the language constructs for binary and unary operators.
//! They are used by both the parser (to create surface syntax) and the type checker
//! (to elaborate into core primitives).

/// Operator precedence level (higher value = binds tighter).
///
/// This type encapsulates the precedence value and only allows comparisons,
/// preventing direct access to the internal numeric representation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Precedence(u8);

impl Precedence {
    /// Lowest precedence level (used at the top of expression parsing).
    pub const MIN: Self = Self(1);

    /// Create a precedence level from a numeric value (1-based, higher binds tighter).
    const fn new(level: u8) -> Self {
        Self(level)
    }

    /// Increment precedence by one level (for left-associative operators).
    #[must_use]
    pub const fn next_level(self) -> Self {
        Self(self.0 + 1)
    }
}

/// Operator associativity for precedence climbing in the parser.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Assoc {
    /// Left-associative (e.g., a + b + c = (a + b) + c)
    Left,
    /// Right-associative (e.g., a :: b :: c = a :: (b :: c))
    Right,
}

/// Binary operator.
///
/// These are the operators that can appear in infix position in the source language.
/// Each maps to a core primitive during elaboration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
    BitAnd,
    BitOr,
}

impl BinOp {
    /// Operator precedence (higher value = binds tighter).
    ///
    /// Used by the parser's precedence climbing algorithm.
    pub const fn precedence(self) -> Precedence {
        let level = match self {
            Self::BitOr => 1,
            Self::BitAnd => 2,
            Self::Eq | Self::Ne | Self::Lt | Self::Gt | Self::Le | Self::Ge => 3,
            Self::Add | Self::Sub => 4,
            Self::Mul | Self::Div => 5,
        };
        Precedence::new(level)
    }

    /// Operator associativity.
    ///
    /// All binary operators in Splic are left-associative.
    pub const fn assoc(self) -> Assoc {
        Assoc::Left
    }
}

/// Unary operator.
///
/// These are the operators that can appear in prefix position in the source language.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnOp {
    Not,
}

impl UnOp {
    /// Operator precedence (higher value = binds tighter).
    ///
    /// Unary operators bind tighter than all binary operators.
    pub const fn precedence(self) -> Precedence {
        Precedence::new(6)
    }
}
