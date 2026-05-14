use std::fmt;

use super::{Arm, Global, GlobalDef, Let, Name, Pat, Program, Term};
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

impl<'names> Let<'names, '_> {
    /// Write the contents of a `{ }` block for this let-chain, without the
    /// surrounding braces. Each binding occupies one line; the tail expression
    /// is written last and followed by a newline.
    fn fmt_sequence(
        &self,
        env: &mut Env<&'names Name>,
        indent: usize,
        f: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        write_indent(f, indent)?;
        write!(f, "let {}@{}: ", self.name, env.depth())?;
        self.ty.fmt_expr(env, indent, f)?;
        write!(f, " = ")?;
        self.expr.fmt_expr(env, indent, f)?;
        writeln!(f, ";")?;
        env.push(self.name);
        let result = match self.body {
            Term::Let(inner) => inner.fmt_sequence(env, indent, f),
            tail => {
                write_indent(f, indent)?;
                tail.fmt_expr(env, indent, f)?;
                writeln!(f)
            }
        };
        env.pop();
        result
    }
}

impl<'names> Term<'names, '_> {
    /// Print `self` in **expression position** (no leading indentation).
    ///
    /// - `Let` is wrapped in `{ }` (block expression; `let` is only valid inside blocks).
    /// - `Match` is wrapped in `( )` (parenthesised expression).
    /// - Everything else is printed inline.
    fn fmt_expr(
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

            // ── Let binding — block expression ────────────────────────────────────
            Term::Let(let_) => {
                writeln!(f, "{{")?;
                let_.fmt_sequence(env, indent + 1, f)?;
                write_indent(f, indent)?;
                write!(f, "}}")
            }

            // ── Match ────────────────────────────────────────────────────────────
            Term::Match(match_) => {
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
                let mut env: Env<&Name> = Env::new();
                write!(f, "def {}: ", self.name)?;
                meta.ty.fmt_expr(&mut env, 0, f)?;
                write!(f, " = ")?;
                meta.body.fmt_expr(&mut env, 0, f)?;
            }
            Global::CodeFn(codefn) => {
                let mut env: Env<&Name> = Env::with_capacity(codefn.params.len());
                write!(f, "code def {}(", self.name)?;
                fmt_params(codefn.params, &mut env, 0, f)?;
                write!(f, ") -> ")?;
                codefn.ret_ty.fmt_expr(&mut env, 0, f)?;
                write!(f, " = ")?;
                codefn.body.fmt_expr(&mut env, 0, f)?;
            }
            Global::CodeConst(c) => {
                let mut env: Env<&Name> = Env::new();
                write!(f, "code def {}: ", self.name)?;
                c.ty.fmt_expr(&mut env, 0, f)?;
                write!(f, " = ")?;
                c.body.fmt_expr(&mut env, 0, f)?;
            }
        }
        writeln!(f, ";")
    }
}
