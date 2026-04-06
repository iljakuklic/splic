pub mod pretty;
mod prim;
pub mod value;

pub mod alpha_eq;
pub use crate::common::{Name, Phase, de_bruijn};
pub use alpha_eq::alpha_eq;
pub use prim::{IntType, IntWidth, Prim};

/// Match pattern in the core IR
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pat<'n> {
    Lit(u64),
    Bind(&'n Name), // named binding
    Wildcard,       // _ pattern
}

impl<'n> Pat<'n> {
    /// Return the name bound by this pattern, if any.
    pub const fn bound_name(&self) -> Option<&'n Name> {
        match self {
            Pat::Bind(name) => Some(*name),
            Pat::Lit(_) | Pat::Wildcard => None,
        }
    }
}

/// Match arm
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Arm<'n, 't> {
    pub pat: Pat<'n>,
    pub body: &'t Term<'n, 't>,
}

/// Elaborated top-level function definition.
#[derive(Debug)]
pub struct Function<'n, 't> {
    pub name: &'n Name,
    /// Function type: phase, params, and return type.
    pub ty: &'t Pi<'n, 't>,
    pub body: &'t Term<'n, 't>,
}

impl<'n, 'a> Function<'n, 'a> {
    /// Return the function's Pi type.
    pub const fn pi(&self) -> &Pi<'n, 'a> {
        self.ty
    }
}

/// Elaborated program: a sequence of top-level function definitions
#[derive(Debug)]
pub struct Program<'n, 'a> {
    pub functions: &'a [Function<'n, 'a>],
}

/// Function or primitive application: `func(args...)`
///
/// `func` may be any term yielding a function type — most commonly:
/// - `Term::Global(name)` for top-level function calls
/// - `Term::Prim(p)` for built-in primitive operations
/// - any expression for higher-order calls
///
/// An empty `args` slice represents a zero-argument call and is distinct from
/// a bare reference to `func`.
#[derive(Debug, PartialEq, Eq)]
pub struct App<'n, 'a> {
    pub func: &'a Term<'n, 'a>,
    pub args: &'a [&'a Term<'n, 'a>],
}

/// Dependent function type: `fn(params...) -> body_ty`
///
/// `phase` distinguishes meta-level (`fn`) from object-level (`code fn`) functions.
/// This allows the globals table to store `&Term` directly, unifying type lookup
/// for globals and locals.
#[derive(Debug, PartialEq, Eq)]
pub struct Pi<'n, 'a> {
    pub params: &'a [(&'n Name, &'a Term<'n, 'a>)], // (name, type) pairs
    pub body_ty: &'a Term<'n, 'a>,
    pub phase: Phase,
}

/// Lambda abstraction: |params...| body
#[derive(Debug, PartialEq, Eq)]
pub struct Lam<'n, 'a> {
    pub params: &'a [(&'n Name, &'a Term<'n, 'a>)], // (name, type) pairs
    pub body: &'a Term<'n, 'a>,
}

/// Let binding with explicit type annotation and a body.
#[derive(Debug, PartialEq, Eq)]
pub struct Let<'n, 'a> {
    pub name: &'n Name,
    pub ty: &'a Term<'n, 'a>,
    pub expr: &'a Term<'n, 'a>,
    pub body: &'a Term<'n, 'a>,
}

/// Pattern match.
#[derive(Debug, PartialEq, Eq)]
pub struct Match<'n, 'a> {
    pub scrutinee: &'a Term<'n, 'a>,
    pub arms: &'a [Arm<'n, 'a>],
}

/// Core term / type (terms and types are unified)
#[derive(Debug, PartialEq, Eq, derive_more::From)]
pub enum Term<'n, 'a> {
    /// Local variable, identified by De Bruijn index (0 = innermost binder)
    Var(de_bruijn::Ix),
    /// Built-in type or operation (not applied)
    #[from]
    Prim(Prim),
    /// Numeric literal with its integer type
    Lit(u64, IntType),
    /// Global function reference
    Global(&'n Name),
    /// Function or primitive application: func(args...)
    #[from]
    App(App<'n, 'a>),
    /// Dependent function type: fn(x: A) -> B
    #[from]
    Pi(Pi<'n, 'a>),
    /// Lambda abstraction: |x: A| body
    #[from]
    Lam(Lam<'n, 'a>),
    /// Lift: [[T]] — meta type representing object-level code of type T
    Lift(&'a Self),
    /// Quotation: #(t) — produce object-level code from a meta expression
    Quote(&'a Self),
    /// Splice: $(t) — run meta code and insert result into object context
    Splice(&'a Self),
    /// Let binding with explicit type annotation and a body
    #[from]
    Let(Let<'n, 'a>),
    /// Pattern match
    #[from]
    Match(Match<'n, 'a>),
}

impl Term<'static, 'static> {
    // Integer types at meta phase
    pub const U0_META: Self = Self::Prim(Prim::IntTy(IntType::U0_META));
    pub const U1_META: Self = Self::Prim(Prim::IntTy(IntType::U1_META));
    pub const U8_META: Self = Self::Prim(Prim::IntTy(IntType::U8_META));
    pub const U16_META: Self = Self::Prim(Prim::IntTy(IntType::U16_META));
    pub const U32_META: Self = Self::Prim(Prim::IntTy(IntType::U32_META));
    pub const U64_META: Self = Self::Prim(Prim::IntTy(IntType::U64_META));

    // Integer types at object phase
    pub const U0_OBJ: Self = Self::Prim(Prim::IntTy(IntType::U0_OBJ));
    pub const U1_OBJ: Self = Self::Prim(Prim::IntTy(IntType::U1_OBJ));
    pub const U8_OBJ: Self = Self::Prim(Prim::IntTy(IntType::U8_OBJ));
    pub const U16_OBJ: Self = Self::Prim(Prim::IntTy(IntType::U16_OBJ));
    pub const U32_OBJ: Self = Self::Prim(Prim::IntTy(IntType::U32_OBJ));
    pub const U64_OBJ: Self = Self::Prim(Prim::IntTy(IntType::U64_OBJ));

    // Universes
    pub const TYPE: Self = Self::Prim(Prim::U(Phase::Meta));
    pub const VM_TYPE: Self = Self::Prim(Prim::U(Phase::Object));

    /// Return the static integer-type term for the given width and phase.
    pub const fn int_ty(width: IntWidth, phase: Phase) -> &'static Self {
        match (width, phase) {
            (IntWidth::U0, Phase::Meta) => &Self::U0_META,
            (IntWidth::U1, Phase::Meta) => &Self::U1_META,
            (IntWidth::U8, Phase::Meta) => &Self::U8_META,
            (IntWidth::U16, Phase::Meta) => &Self::U16_META,
            (IntWidth::U32, Phase::Meta) => &Self::U32_META,
            (IntWidth::U64, Phase::Meta) => &Self::U64_META,
            (IntWidth::U0, Phase::Object) => &Self::U0_OBJ,
            (IntWidth::U1, Phase::Object) => &Self::U1_OBJ,
            (IntWidth::U8, Phase::Object) => &Self::U8_OBJ,
            (IntWidth::U16, Phase::Object) => &Self::U16_OBJ,
            (IntWidth::U32, Phase::Object) => &Self::U32_OBJ,
            (IntWidth::U64, Phase::Object) => &Self::U64_OBJ,
        }
    }

    /// Return the static u1 term for the given phase.
    pub const fn u1_ty(phase: Phase) -> &'static Self {
        match phase {
            Phase::Meta => &Self::U1_META,
            Phase::Object => &Self::U1_OBJ,
        }
    }

    /// Return the universe term for the given phase (`Type` or `VmType`).
    pub const fn universe(phase: Phase) -> &'static Self {
        match phase {
            Phase::Meta => &Self::TYPE,
            Phase::Object => &Self::VM_TYPE,
        }
    }
}

impl<'n, 'a> Term<'n, 'a> {
    pub const fn new_app(func: &'a Self, args: &'a [&'a Self]) -> Self {
        Self::App(App { func, args })
    }

    pub const fn new_let(name: &'n Name, ty: &'a Self, expr: &'a Self, body: &'a Self) -> Self {
        Self::Let(Let {
            name,
            ty,
            expr,
            body,
        })
    }

    pub const fn new_match(scrutinee: &'a Self, arms: &'a [Arm<'n, 'a>]) -> Self {
        Self::Match(Match { scrutinee, arms })
    }
}
