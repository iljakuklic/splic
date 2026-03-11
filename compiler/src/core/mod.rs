use crate::parser::ast::Phase;

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

/// Integer type: width + phase (meta vs. object)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct IntType {
    pub width: IntWidth,
    pub phase: Phase,
}

impl IntType {
    pub fn new(width: IntWidth, phase: Phase) -> Self {
        IntType { width, phase }
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
    // Comparison (return U1 at the same phase)
    Eq(IntType),
    Ne(IntType),
    Lt(IntType),
    Gt(IntType),
    Le(IntType),
    Ge(IntType),
}

/// De Bruijn level (counts from the outermost binder)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Lvl(pub usize);

impl Lvl {
    pub fn new(n: usize) -> Self {
        Lvl(n)
    }

    pub fn succ(self) -> Self {
        Lvl(self.0 + 1)
    }
}

/// Head of an application: either a top-level function or a primitive op
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Head<'a> {
    Global(&'a str), // resolved top-level function name
    Prim(Prim),      // built-in operation with resolved width
}

/// Match pattern in the core IR
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pat<'a> {
    Lit(u64),
    Bind(&'a str), // named binding
    Wildcard,      // _ pattern
}

/// Match arm
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Arm<'a> {
    pub pat: Pat<'a>,
    pub body: &'a Term<'a>,
}

/// Top-level function signature (stored in the globals table during elaboration)
#[derive(Debug)]
pub struct FunSig<'a> {
    pub params: &'a [(&'a str, &'a Term<'a>)], // (name, type) pairs
    pub ret_ty: &'a Term<'a>,
    pub phase: Phase,
}

/// Elaborated top-level function definition
#[derive(Debug)]
pub struct Function<'a> {
    pub name: &'a str,
    pub sig: FunSig<'a>,
    pub body: &'a Term<'a>,
}

/// Elaborated program: a sequence of top-level function definitions
#[derive(Debug)]
pub struct Program<'a> {
    pub functions: &'a [Function<'a>],
}

/// Core term / type (terms and types are unified)
#[derive(Debug, PartialEq, Eq)]
pub enum Term<'a> {
    /// Local variable, identified by De Bruijn level
    Var(Lvl),
    /// Built-in type or operation
    Prim(Prim),
    /// Numeric literal
    Lit(u64),
    /// Application of a global function or primitive operation to arguments
    App {
        head: Head<'a>,
        args: &'a [&'a Term<'a>],
    },
    /// Lift: [[T]] — meta type representing object-level code of type T
    Lift(&'a Term<'a>),
    /// Quotation: #(t) — produce object-level code from a meta expression
    Quote(&'a Term<'a>),
    /// Splice: $(t) — run meta code and insert result into object context
    Splice(&'a Term<'a>),
    /// Let binding with explicit type annotation and a body
    Let {
        name: &'a str,
        ty: &'a Term<'a>,
        expr: &'a Term<'a>,
        body: &'a Term<'a>,
    },
    /// Pattern match
    Match {
        scrutinee: &'a Term<'a>,
        arms: &'a [Arm<'a>],
    },
}
