#[derive(Clone, Copy)]
pub enum Phase {
    Meta,
    Object,
}

pub struct Name<'a>(pub &'a str);

#[derive(Clone, Copy)]
pub enum PrimTy {
    U0,
    U1,
    U8,
    U16,
    U32,
    U64,
    Type,
    VmType,
}

#[derive(Clone, Copy)]
pub enum BinaryOp {
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

#[derive(Clone, Copy)]
pub enum UnaryOp {
    Not,
}

pub enum Pat<'a> {
    Wildcard,
    Name(Name<'a>),
    Lit(u64),
}

pub struct MatchArm<'a> {
    pub pat: &'a Pat<'a>,
    pub body: &'a Term<'a>,
}

pub struct Let<'a> {
    pub name: Name<'a>,
    pub ty: Option<&'a Term<'a>>,
    pub expr: &'a Term<'a>,
}

pub struct Param<'a> {
    pub name: Name<'a>,
    pub ty: &'a Term<'a>,
}

pub struct Function<'a> {
    pub phase: Phase,
    pub name: Name<'a>,
    pub params: &'a [Param<'a>],
    pub ret_ty: &'a Term<'a>,
    pub body: &'a Term<'a>,
}

pub struct Program<'a> {
    pub functions: &'a [Function<'a>],
}

pub enum Term<'a> {
    Lit(u64),
    Var(Name<'a>),
    App {
        func: &'a Term<'a>,
        args: &'a [&'a Term<'a>],
    },
    Binary {
        op: BinaryOp,
        lhs: &'a Term<'a>,
        rhs: &'a Term<'a>,
    },
    Unary {
        op: UnaryOp,
        arg: &'a Term<'a>,
    },
    Quote(&'a Term<'a>),
    Splice(&'a Term<'a>),
    Lift(&'a Term<'a>),
    Match {
        scrutinee: &'a Term<'a>,
        arms: &'a [MatchArm<'a>],
    },
    Block {
        stmts: &'a [Let<'a>],
        expr: &'a Term<'a>,
    },
}
