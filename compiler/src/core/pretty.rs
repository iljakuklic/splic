use std::fmt;

use crate::parser::ast::Phase;

use super::{Arm, Function, Head, IntWidth, Pat, Prim, Program, Term};

// ── Helpers ───────────────────────────────────────────────────────────────────

fn write_indent(f: &mut fmt::Formatter<'_>, depth: usize) -> fmt::Result {
    for _ in 0..depth {
        write!(f, "    ")?;
    }
    Ok(())
}

const fn prim_int_width(width: IntWidth) -> &'static str {
    match width {
        IntWidth::U0 => "u0",
        IntWidth::U1 => "u1",
        IntWidth::U8 => "u8",
        IntWidth::U16 => "u16",
        IntWidth::U32 => "u32",
        IntWidth::U64 => "u64",
    }
}

/// Returns the infix operator symbol for a binary primitive, or `None` if the
/// primitive is not a binary infix operator.
const fn binop_symbol(prim: Prim) -> Option<&'static str> {
    match prim {
        Prim::Add(_) => Some("+"),
        Prim::Sub(_) => Some("-"),
        Prim::Mul(_) => Some("*"),
        Prim::Div(_) => Some("/"),
        Prim::BitAnd(_) => Some("&"),
        Prim::BitOr(_) => Some("|"),
        Prim::Eq(_) => Some("=="),
        Prim::Ne(_) => Some("!="),
        Prim::Lt(_) => Some("<"),
        Prim::Gt(_) => Some(">"),
        Prim::Le(_) => Some("<="),
        Prim::Ge(_) => Some(">="),
        Prim::IntTy(_) | Prim::U(_) | Prim::BitNot(_) | Prim::Embed(_) => None,
    }
}

/// Whether a term needs parentheses when used as an atomic sub-expression
/// (i.e. as an argument to an application or operand to an operator).
const fn needs_parens(term: &Term<'_>) -> bool {
    match term {
        Term::App {
            head: Head::Prim(p),
            args,
        } => {
            // Binary infix ops need parens; unary BitNot does not.
            binop_symbol(*p).is_some() && args.len() == 2
        }
        Term::Var(_)
        | Term::Prim(_)
        | Term::Lit(_)
        | Term::App { .. }
        | Term::Lift(_)
        | Term::Quote(_)
        | Term::Splice(_)
        | Term::Let { .. }
        | Term::Match { .. } => false,
    }
}

// ── Core formatting ───────────────────────────────────────────────────────────

/// Print `term` in **statement position**: emits leading indentation, then
/// the term content. `Let` and `Match` are printed without an enclosing `{ }`
/// (the caller is responsible for any surrounding braces).
fn fmt_term<'a>(
    term: &Term<'a>,
    env: &mut Vec<&'a str>,
    indent: usize,
    f: &mut fmt::Formatter<'_>,
) -> fmt::Result {
    match term {
        // Let and Match manage their own indentation internally.
        Term::Let { .. } | Term::Match { .. } => fmt_term_inline(term, env, indent, f),
        // Everything else gets a leading indent.
        Term::Var(_)
        | Term::Prim(_)
        | Term::Lit(_)
        | Term::App { .. }
        | Term::Lift(_)
        | Term::Quote(_)
        | Term::Splice(_) => {
            write_indent(f, indent)?;
            fmt_term_inline(term, env, indent, f)
        }
    }
}

/// Print `term` **inline** (no leading indentation). Used when the term
/// appears as a sub-expression — inside `#(...)`, as a binop operand, etc.
///
/// `indent` is the current block depth, used only when this term itself opens
/// a new indented block (e.g. `Let` / `Match`).
fn fmt_term_inline<'a>(
    term: &Term<'a>,
    env: &mut Vec<&'a str>,
    indent: usize,
    f: &mut fmt::Formatter<'_>,
) -> fmt::Result {
    match term {
        // ── Variable ─────────────────────────────────────────────────────────
        Term::Var(lvl) => {
            let name = *env.get(lvl.0).expect("De Bruijn level in env bounds");
            write!(f, "{name}@{}", lvl.0)
        }

        // ── Literal ──────────────────────────────────────────────────────────
        Term::Lit(n) => write!(f, "{n}"),

        // ── Primitive type / universe ─────────────────────────────────────────
        Term::Prim(p) => fmt_prim_ty(*p, f),

        // ── Application ──────────────────────────────────────────────────────
        Term::App { head, args } => fmt_app(head, args, env, indent, f),

        // ── Lift / Quote / Splice ─────────────────────────────────────────────
        Term::Lift(inner) => {
            write!(f, "[[")?;
            fmt_atom(inner, env, indent, f)?;
            write!(f, "]]")
        }
        Term::Quote(inner) => {
            write!(f, "#(")?;
            fmt_atom(inner, env, indent, f)?;
            write!(f, ")")
        }
        Term::Splice(inner) => {
            write!(f, "$(")?;
            fmt_atom(inner, env, indent, f)?;
            write!(f, ")")
        }

        // ── Let binding ───────────────────────────────────────────────────────
        // In statement position: print as a flat let-chain without extra braces.
        Term::Let {
            name,
            ty,
            expr,
            body,
        } => {
            let lvl = env.len();
            write_indent(f, indent)?;
            write!(f, "let {name}@{lvl}: ")?;
            fmt_atom(ty, env, indent, f)?;
            write!(f, " = ")?;
            fmt_expr(expr, env, indent, f)?;
            writeln!(f, ";")?;
            env.push(name);
            fmt_term(body, env, indent, f)?;
            env.pop();
            Ok(())
        }

        // ── Match ─────────────────────────────────────────────────────────────
        Term::Match { scrutinee, arms } => {
            write_indent(f, indent)?;
            write!(f, "match ")?;
            fmt_atom(scrutinee, env, indent, f)?;
            writeln!(f, " {{")?;
            for arm in *arms {
                fmt_arm(arm, env, indent + 1, f)?;
            }
            write_indent(f, indent)?;
            write!(f, "}}")
        }
    }
}

/// Print `term` in **expression position** (inline, no leading indent).
///
/// Unlike `fmt_term_inline`, wraps `Let` and `Match` in `{ }` so they are
/// syntactically valid as sub-expressions.
fn fmt_expr<'a>(
    term: &Term<'a>,
    env: &mut Vec<&'a str>,
    indent: usize,
    f: &mut fmt::Formatter<'_>,
) -> fmt::Result {
    match term {
        Term::Let { .. } | Term::Match { .. } => {
            writeln!(f, "{{")?;
            fmt_term(term, env, indent + 1, f)?;
            writeln!(f)?;
            write_indent(f, indent)?;
            write!(f, "}}")
        }
        Term::Var(_)
        | Term::Prim(_)
        | Term::Lit(_)
        | Term::App { .. }
        | Term::Lift(_)
        | Term::Quote(_)
        | Term::Splice(_) => fmt_term_inline(term, env, indent, f),
    }
}

/// Print `term` as an atomic sub-expression, adding parentheses when needed
/// to preserve syntactic validity (i.e. for binary operator applications).
fn fmt_atom<'a>(
    term: &Term<'a>,
    env: &mut Vec<&'a str>,
    indent: usize,
    f: &mut fmt::Formatter<'_>,
) -> fmt::Result {
    if needs_parens(term) {
        write!(f, "(")?;
        fmt_term_inline(term, env, indent, f)?;
        write!(f, ")")
    } else {
        fmt_expr(term, env, indent, f)
    }
}

/// Print a `Prim` that appears in type/universe position (as `Term::Prim`).
fn fmt_prim_ty(prim: Prim, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match prim {
        Prim::IntTy(it) => write!(f, "{}", prim_int_width(it.width)),
        Prim::U(Phase::Meta) => write!(f, "Type"),
        Prim::U(Phase::Object) => write!(f, "VmType"),
        // Arithmetic/comparison prims should only appear as Head::Prim inside
        // App, never as standalone Term::Prim.
        p @ (Prim::Add(_)
        | Prim::Sub(_)
        | Prim::Mul(_)
        | Prim::Div(_)
        | Prim::BitAnd(_)
        | Prim::BitOr(_)
        | Prim::BitNot(_)
        | Prim::Embed(_)
        | Prim::Eq(_)
        | Prim::Ne(_)
        | Prim::Lt(_)
        | Prim::Gt(_)
        | Prim::Le(_)
        | Prim::Ge(_)) => panic!("unexpected primitive in type position: {p:?}"),
    }
}

/// Print an application.
fn fmt_app<'a>(
    head: &Head<'a>,
    args: &[&'a Term<'a>],
    env: &mut Vec<&'a str>,
    indent: usize,
    f: &mut fmt::Formatter<'_>,
) -> fmt::Result {
    match head {
        // ── Global function call ──────────────────────────────────────────────
        Head::Global(name) => {
            write!(f, "{name}(")?;
            for (i, arg) in args.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                fmt_expr(arg, env, indent, f)?;
            }
            write!(f, ")")
        }

        // ── Primitive operation ───────────────────────────────────────────────
        Head::Prim(prim) => {
            #[allow(clippy::indexing_slicing)]
            if let Some(sym) = binop_symbol(*prim) {
                // Binary infix operator — exactly 2 args.
                fmt_atom(args[0], env, indent, f)?;
                write!(f, " {sym} ")?;
                fmt_atom(args[1], env, indent, f)
            } else {
                match prim {
                    Prim::BitNot(_) => {
                        write!(f, "!")?;
                        fmt_atom(args[0], env, indent, f)
                    }
                    Prim::Embed(_) => {
                        // Transparent: just print the argument.
                        fmt_atom(args[0], env, indent, f)
                    }
                    // Type-level prims should not appear as App heads.
                    p @ (Prim::IntTy(_)
                    | Prim::U(_)
                    | Prim::Add(_)
                    | Prim::Sub(_)
                    | Prim::Mul(_)
                    | Prim::Div(_)
                    | Prim::BitAnd(_)
                    | Prim::BitOr(_)
                    | Prim::Eq(_)
                    | Prim::Ne(_)
                    | Prim::Lt(_)
                    | Prim::Gt(_)
                    | Prim::Le(_)
                    | Prim::Ge(_)) => panic!("unexpected primitive as application head: {p:?}"),
                }
            }
        }
    }
}

/// Print a single match arm.
fn fmt_arm<'a>(
    arm: &Arm<'a>,
    env: &mut Vec<&'a str>,
    indent: usize,
    f: &mut fmt::Formatter<'_>,
) -> fmt::Result {
    write_indent(f, indent)?;
    match &arm.pat {
        Pat::Lit(n) => {
            write!(f, "{n} => ")?;
            fmt_expr(arm.body, env, indent, f)?;
        }
        Pat::Wildcard => {
            write!(f, "_ => ")?;
            fmt_expr(arm.body, env, indent, f)?;
        }
        Pat::Bind(name) => {
            let lvl = env.len();
            write!(f, "{name}@{lvl} => ")?;
            env.push(name);
            fmt_expr(arm.body, env, indent, f)?;
            env.pop();
        }
    }
    writeln!(f, ",")
}

// ── Display impls ─────────────────────────────────────────────────────────────

impl fmt::Display for Program<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, func) in self.functions.iter().enumerate() {
            if i > 0 {
                writeln!(f)?;
            }
            write!(f, "{func}")?;
        }
        Ok(())
    }
}

impl fmt::Display for Function<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Build the name environment for the body: one entry per parameter.
        let mut env: Vec<&str> = Vec::with_capacity(self.sig.params.len());

        // Phase prefix.
        match self.sig.phase {
            Phase::Object => write!(f, "code ")?,
            Phase::Meta => {}
        }
        write!(f, "fn {}(", self.name)?;

        // Parameters: types are printed with the env as built so far (dependent
        // function types: earlier params are in scope for later param types).
        for (i, (name, ty)) in self.sig.params.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{name}@{i}: ")?;
            fmt_prim_ty_or_term(ty, &mut env, f)?;
            env.push(name);
        }

        write!(f, ") -> ")?;
        fmt_prim_ty_or_term(self.sig.ret_ty, &mut env, f)?;
        writeln!(f, " {{")?;

        // Body in statement position at indent depth 1.
        fmt_term(self.body, &mut env, 1, f)?;
        writeln!(f)?;
        writeln!(f, "}}")
    }
}

/// Print a term that appears in a type/signature position. For the common case
/// of a plain `Prim`, delegates to `fmt_prim_ty`. For anything more complex
/// (e.g. `Lift`, `App`) the full inline printer is used.
fn fmt_prim_ty_or_term<'a>(
    term: &Term<'a>,
    env: &mut Vec<&'a str>,
    f: &mut fmt::Formatter<'_>,
) -> fmt::Result {
    match term {
        Term::Prim(p) => fmt_prim_ty(*p, f),
        Term::Var(_)
        | Term::Lit(_)
        | Term::App { .. }
        | Term::Lift(_)
        | Term::Quote(_)
        | Term::Splice(_)
        | Term::Let { .. }
        | Term::Match { .. } => fmt_atom(term, env, 0, f),
    }
}
