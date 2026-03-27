use std::fmt;

use crate::parser::ast::Phase;

use super::{Arm, Function, Pat, Program, Term};

// ── Helpers ───────────────────────────────────────────────────────────────────

fn write_indent(f: &mut fmt::Formatter<'_>, depth: usize) -> fmt::Result {
    write!(f, "{:width$}", "", width = depth * 4)
}

// ── Core formatting ───────────────────────────────────────────────────────────

impl<'a> Term<'a> {
    /// Print `self` in **statement position**: emits leading indentation, then
    /// the term content. `Let` and `Match` are printed without an enclosing `{ }`
    /// (the caller is responsible for any surrounding braces).
    fn fmt_term(
        &self,
        env: &mut Vec<&'a str>,
        indent: usize,
        f: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        match self {
            // Let and Match manage their own indentation internally.
            Term::Let(_) | Term::Match(_) => self.fmt_term_inline(env, indent, f),
            // Everything else gets a leading indent.
            Term::Var(_)
            | Term::Prim(_)
            | Term::Lit(..)
            | Term::Global(_)
            | Term::App(_)
            | Term::Pi(_)
            | Term::Lam(_)
            | Term::Lift(_)
            | Term::Quote(_)
            | Term::Splice(_) => {
                write_indent(f, indent)?;
                self.fmt_term_inline(env, indent, f)
            }
        }
    }

    /// Print `self` **inline** (no leading indentation). Used when the term
    /// appears as a sub-expression — inside `#(...)`, as an argument, etc.
    ///
    /// `indent` is the current block depth, used only when this term itself opens
    /// a new indented block (e.g. `Let` / `Match`).
    fn fmt_term_inline(
        &self,
        env: &mut Vec<&'a str>,
        indent: usize,
        f: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        match self {
            // ── Variable ─────────────────────────────────────────────────────────
            Term::Var(ix) => {
                let i = env
                    .len()
                    .checked_sub(1 + ix.0)
                    .expect("De Bruijn index out of environment bounds");
                let name = env
                    .get(i)
                    .expect("De Bruijn index out of environment bounds");
                write!(f, "{name}@{i}")
            }

            // ── Literal ──────────────────────────────────────────────────────────
            Term::Lit(n, ty) => write!(f, "{n}_{}", ty.width),

            // ── Primitive type / universe ─────────────────────────────────────────
            Term::Prim(p) => write!(f, "{p}"),

            // ── Global reference ──────────────────────────────────────────────────
            Term::Global(name) => write!(f, "{name}"),

            // ── Application ───────────────────────────────────────────────────────
            Term::App(app) => {
                app.func.fmt_expr(env, indent, f)?;
                write!(f, "(")?;
                for (i, arg) in app.args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    arg.fmt_expr(env, indent, f)?;
                }
                write!(f, ")")
            }

            // ── Pi type ───────────────────────────────────────────────────────────
            Term::Pi(pi) => {
                let env_before = env.len();
                write!(f, "fn(")?;
                for (i, &(name, ty)) in pi.params.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    if name == "_" {
                        write!(f, "_: ")?;
                    } else {
                        write!(f, "{}@{}: ", name, env.len())?;
                    }
                    ty.fmt_expr(env, indent, f)?;
                    env.push(name);
                }
                write!(f, ") -> ")?;
                pi.body_ty.fmt_expr(env, indent, f)?;
                env.truncate(env_before);
                Ok(())
            }

            // ── Lambda ────────────────────────────────────────────────────────────
            Term::Lam(lam) => {
                let env_before = env.len();
                write!(f, "|")?;
                for (i, &(name, ty)) in lam.params.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}@{}: ", name, env.len())?;
                    ty.fmt_expr(env, indent, f)?;
                    env.push(name);
                }
                write!(f, "| ")?;
                lam.body.fmt_expr(env, indent, f)?;
                env.truncate(env_before);
                Ok(())
            }

            // ── Lift / Quote / Splice ─────────────────────────────────────────────
            Term::Lift(inner) => {
                write!(f, "[[")?;
                inner.fmt_expr(env, indent, f)?;
                write!(f, "]]")
            }
            Term::Quote(inner) => {
                write!(f, "#(")?;
                inner.fmt_expr(env, indent, f)?;
                write!(f, ")")
            }
            Term::Splice(inner) => {
                write!(f, "$(")?;
                inner.fmt_expr(env, indent, f)?;
                write!(f, ")")
            }

            // ── Let binding ───────────────────────────────────────────────────────
            // In statement position: print as a flat let-chain without extra braces.
            Term::Let(let_) => {
                write_indent(f, indent)?;
                write!(f, "let {}@{}: ", let_.name, env.len())?;
                let_.ty.fmt_expr(env, indent, f)?;
                write!(f, " = ")?;
                let_.expr.fmt_expr(env, indent, f)?;
                writeln!(f, ";")?;
                env.push(let_.name);
                let_.body.fmt_term(env, indent, f)?;
                env.pop();
                Ok(())
            }

            // ── Match ─────────────────────────────────────────────────────────────
            Term::Match(match_) => {
                write_indent(f, indent)?;
                write!(f, "match ")?;
                match_.scrutinee.fmt_expr(env, indent, f)?;
                writeln!(f, " {{")?;
                for arm in match_.arms {
                    arm.fmt_arm(env, indent + 1, f)?;
                }
                write_indent(f, indent)?;
                write!(f, "}}")
            }
        }
    }

    /// Print `self` in **expression position** (inline, no leading indent).
    ///
    /// Unlike `fmt_term_inline`, wraps `Let` and `Match` in `{ }` so they are
    /// syntactically valid as sub-expressions.
    fn fmt_expr(
        &self,
        env: &mut Vec<&'a str>,
        indent: usize,
        f: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        match self {
            Term::Let(_) | Term::Match(_) => {
                writeln!(f, "{{")?;
                self.fmt_term(env, indent + 1, f)?;
                writeln!(f)?;
                write_indent(f, indent)?;
                write!(f, "}}")
            }
            Term::Var(_)
            | Term::Prim(_)
            | Term::Lit(..)
            | Term::Global(_)
            | Term::App(_)
            | Term::Pi(_)
            | Term::Lam(_)
            | Term::Lift(_)
            | Term::Quote(_)
            | Term::Splice(_) => self.fmt_term_inline(env, indent, f),
        }
    }
}

impl<'a> Arm<'a> {
    /// Print a single match arm.
    fn fmt_arm(
        &self,
        env: &mut Vec<&'a str>,
        indent: usize,
        f: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        write_indent(f, indent)?;
        match &self.pat {
            Pat::Lit(n) => write!(f, "{n} => ")?,
            Pat::Wildcard => write!(f, "_ => ")?,
            Pat::Bind(name) => {
                let lvl = env.len();
                write!(f, "{name}@{lvl} => ")?;
                env.push(name);
                self.body.fmt_expr(env, indent, f)?;
                env.pop();
                return writeln!(f, ",");
            }
        }
        self.body.fmt_expr(env, indent, f)?;
        writeln!(f, ",")
    }
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
        let pi = self.pi();

        // Build the name environment for the body: one entry per parameter.
        let mut env: Vec<&str> = Vec::with_capacity(pi.params.len());

        // Phase prefix.
        match pi.phase {
            Phase::Object => write!(f, "code ")?,
            Phase::Meta => {}
        }
        write!(f, "fn {}(", self.name)?;

        // Parameters: types are printed with the env as built so far (dependent
        // function types: earlier params are in scope for later param types).
        for (i, (name, ty)) in pi.params.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{name}@{i}: ")?;
            ty.fmt_expr(&mut env, 1, f)?;
            env.push(name);
        }

        write!(f, ") -> ")?;
        pi.body_ty.fmt_expr(&mut env, 1, f)?;
        writeln!(f, " {{")?;

        // Body in statement position at indent depth 1.
        self.body.fmt_term(&mut env, 1, f)?;
        writeln!(f)?;
        writeln!(f, "}}")
    }
}
