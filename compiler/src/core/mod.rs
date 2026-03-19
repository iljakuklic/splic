pub mod pretty;
mod prim;

pub use prim::{IntType, IntWidth, Prim};
pub use crate::parser::ast::Phase;

/// De Bruijn level (counts from the outermost binder)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Lvl(pub usize);

impl Lvl {
    pub const fn new(n: usize) -> Self {
        Self(n)
    }

    #[must_use]
    pub const fn succ(self) -> Self {
        Self(self.0 + 1)
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

impl<'a> Pat<'a> {
    /// Return the name bound by this pattern, if any.
    pub const fn bound_name(&self) -> Option<&'a str> {
        match self {
            Pat::Bind(name) => Some(name),
            Pat::Lit(_) | Pat::Wildcard => None,
        }
    }
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
        args: &'a [&'a Self],
    },
    /// Lift: [[T]] — meta type representing object-level code of type T
    Lift(&'a Self),
    /// Quotation: #(t) — produce object-level code from a meta expression
    Quote(&'a Self),
    /// Splice: $(t) — run meta code and insert result into object context
    Splice(&'a Self),
    /// Let binding with explicit type annotation and a body
    Let {
        name: &'a str,
        ty: &'a Self,
        expr: &'a Self,
        body: &'a Self,
    },
    /// Pattern match
    Match {
        scrutinee: &'a Self,
        arms: &'a [Arm<'a>],
    },
}
