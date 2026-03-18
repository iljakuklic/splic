pub use crate::lexer::Name;

/// Compilation phase
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Phase {
    Meta,
    Object,
}

impl std::fmt::Display for Phase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Meta => f.write_str("meta"),
            Self::Object => f.write_str("object"),
        }
    }
}

/// Binary operator
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
    pub fn precedence(self) -> u8 {
        match self {
            Self::BitOr => 1,
            Self::BitAnd => 2,
            Self::Eq | Self::Ne | Self::Lt | Self::Gt | Self::Le | Self::Ge => 3,
            Self::Add | Self::Sub => 4,
            Self::Mul | Self::Div => 5,
        }
    }

    pub fn assoc(self) -> Assoc {
        Assoc::Left
    }
}

/// Associativity
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Assoc {
    Left,
    Right,
}

/// Unary operator
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnOp {
    Not,
}

impl UnOp {
    pub fn precedence(self) -> u8 {
        6
    }
}

/// Function or operator reference
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FunName<'a> {
    Name(Name<'a>),
    BinOp(BinOp),
    UnOp(UnOp),
}

impl std::fmt::Debug for FunName<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Name(n) => n.fmt(f),
            Self::BinOp(o) => o.fmt(f),
            Self::UnOp(o) => o.fmt(f),
        }
    }
}

#[derive(Debug)]
pub enum Pat<'a> {
    Name(Name<'a>),
    Lit(u64),
}

#[derive(Debug)]
pub struct MatchArm<'a> {
    pub pat: Pat<'a>,
    pub body: &'a Term<'a>,
}

#[derive(Debug)]
pub struct Let<'a> {
    pub name: Name<'a>,
    pub ty: Option<&'a Term<'a>>,
    pub expr: &'a Term<'a>,
}

#[derive(Debug)]
pub struct Param<'a> {
    pub name: Name<'a>,
    pub ty: &'a Term<'a>,
}

#[derive(Debug)]
pub struct Function<'a> {
    pub phase: Phase,
    pub name: Name<'a>,
    pub params: &'a [Param<'a>],
    pub ret_ty: &'a Term<'a>,
    pub body: &'a Term<'a>,
}

#[derive(Debug)]
pub struct Program<'a> {
    pub functions: &'a [Function<'a>],
}

#[derive(Debug)]
pub enum Term<'a> {
    Lit(u64),
    Var(Name<'a>),
    App {
        func: FunName<'a>,
        args: &'a [&'a Self],
    },
    Quote(&'a Self),
    Splice(&'a Self),
    Lift(&'a Self),
    Match {
        scrutinee: &'a Self,
        arms: &'a [MatchArm<'a>],
    },
    Block {
        stmts: &'a [Let<'a>],
        expr: &'a Self,
    },
}
