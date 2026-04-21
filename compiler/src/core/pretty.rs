use std::fmt;

use super::{Arm, Global, GlobalDef, Name, Pat, Program, Term};
use crate::common::env::Env;

// ── Helpers ───────────────────────────────────────────────────────────────────

fn write_indent(f: &mut fmt::Formatter<'_>, depth: usize) -> fmt::Result {
    write!(f, "{:width$}", "", width = depth * 4)
}

/// Write a comma-separated parameter list `name@depth: ty, ...`,
/// pushing each name onto `env` as it is written.
/// `_`-named params are printed as `_: ty` (no level index).
fn fmt_params<'names>(
    params: &[(&'names Name, &Term<'names, '_>)],
    env: &mut Env<&'names Name>,
    indent: usize,
    f: &mut fmt::Formatter<'_>,
) -> fmt::Result {
    for (i, &(name, ty)) in params.iter().enumerate() {
        if i > 0 {
            write!(f, ", ")?;
        }
        if name.as_str() == "_" {
            write!(f, "_: ")?;
        } else {
            write!(f, "{}@{}: ", name, env.depth())?;
        }
        ty.fmt_expr(env, indent, f)?;
        env.push(name);
    }
    Ok(())
}

// ── Core formatting ───────────────────────────────────────────────────────────

impl<'names> Term<'names, '_> {
    /// Print `self` in **statement position**: emits leading indentation, then
    /// the term content. `Let` and `Match` are printed without an enclosing `{ }`
    /// (the caller is responsible for any surrounding braces).
    fn fmt_term(
        &self,
        env: &mut Env<&'names Name>,
        indent: usize,
        f: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        match self {
            // Let and Match manage their own indentation internally.
            Term::Let(_) | Term::Match(_) => self.fmt_term_inline(env, indent, f),
            // Everything else gets a leading indent.
            _ => {
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
        env: &mut Env<&'names Name>,
        indent: usize,
        f: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        match self {
            // ── Variable ─────────────────────────────────────────────────────────
            Term::Var(ix) => {
                let name = &env[*ix];
                let lvl = env.ix_to_lvl(*ix);
                write!(f, "{name}@{lvl}")
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
                let depth_before = env.depth();
                write!(f, "fn(")?;
                fmt_params(pi.params, env, indent, f)?;
                write!(f, ") -> ")?;
                pi.body_ty.fmt_expr(env, indent, f)?;
                env.truncate(depth_before);
                Ok(())
            }

            // ── Lambda ────────────────────────────────────────────────────────────
            Term::Lam(lam) => {
                let depth_before = env.depth();
                write!(f, "lam(")?;
                fmt_params(lam.params, env, indent, f)?;
                write!(f, ") = ")?;
                lam.body.fmt_expr(env, indent, f)?;
                env.truncate(depth_before);
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
                write!(f, "let {}@{}: ", let_.name, env.depth())?;
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
        env: &mut Env<&'names Name>,
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

impl<'names> Arm<'names, '_> {
    /// Print a single match arm.
    fn fmt_arm(
        &self,
        env: &mut Env<&'names Name>,
        indent: usize,
        f: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        write_indent(f, indent)?;
        match &self.pat {
            Pat::Lit(n) => write!(f, "{n} => ")?,
            Pat::Wildcard => write!(f, "_ => ")?,
            Pat::Bind(name) => {
                write!(f, "{name}@{} => ", env.depth())?;
                env.push(*name);
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

impl fmt::Display for Program<'_, '_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, defn) in self.defs.iter().enumerate() {
            if i > 0 {
                writeln!(f)?;
            }
            write!(f, "{defn}")?;
        }
        Ok(())
    }
}

impl fmt::Display for GlobalDef<'_, '_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.global {
            Global::Meta(meta) => {
                // Meta constant: `def name: ty { body }`
                let mut env: Env<&Name> = Env::new();
                write!(f, "def {}: ", self.name)?;
                meta.ty.fmt_expr(&mut env, 1, f)?;
                writeln!(f, " {{")?;
                meta.body.fmt_term(&mut env, 1, f)?;
            }
            Global::CodeFn(codefn) => {
                // Object function: `code def name(params) -> ret { body }`
                let mut env: Env<&Name> = Env::with_capacity(codefn.params.len());
                write!(f, "code def {}(", self.name)?;
                fmt_params(codefn.params, &mut env, 1, f)?;
                write!(f, ") -> ")?;
                codefn.ret_ty.fmt_expr(&mut env, 1, f)?;
                writeln!(f, " {{")?;
                codefn.body.fmt_term(&mut env, 1, f)?;
            }
            Global::CodeConst(c) => {
                // Object constant: `code def name: ty { body }`
                let mut env: Env<&Name> = Env::new();
                write!(f, "code def {}: ", self.name)?;
                c.ty.fmt_expr(&mut env, 1, f)?;
                writeln!(f, " {{")?;
                c.body.fmt_term(&mut env, 1, f)?;
            }
        }

        writeln!(f)?;
        writeln!(f, "}}")
    }
}
