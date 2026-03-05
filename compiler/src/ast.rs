pub use crate::lexer::Name;

#[derive(Clone, Copy, Debug)]
pub enum Phase {
    Meta,
    Object,
}

#[derive(Clone, Copy, Debug)]
pub enum Primitive {
    // Arithmetic ops
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
    Not,

    // Builtin types
    U0,
    U1,
    U8,
    U16,
    U32,
    U64,

    // Universes
    Type(Phase),
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
        func: Name<'a>,
        args: &'a [&'a Term<'a>],
    },
    Prim(Primitive),
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
