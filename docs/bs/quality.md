# Code Quality

## Clippy Philosophy

The project uses Clippy as the primary code quality tool, with a curated set of
lints enabled beyond the defaults — drawn from `clippy::pedantic`,
`clippy::nursery`, and selectively from `clippy::restriction`. The full list
lives in `[workspace.lints]` in `Cargo.toml`, so lints are enforced by IDEs and
`cargo check` without needing to invoke `just clippy` explicitly.

The goal is to have Clippy surface potential issues so the developer can make an
explicit decision: fix the code, or suppress with a targeted `#[expect(...)]`. A
`#[expect]` is not an admission of defeat — it is documentation that the flagged
pattern was reviewed and kept intentionally. It also has the advantage over
`#[allow]` that it becomes a compile error if the lint stops firing, keeping
suppressions tidy as the code evolves.

Lints are selected aggressively, with one guiding principle: a lint should be
enabled if every violation is worth at least a quick look. Lints that are both
high-noise *and* low-value — firing often with suggestions that are rarely
improvements — are left off. The full `clippy::restriction` group is not
bulk-enabled for this reason; many of its lints conflict with idiomatic Rust
(e.g. `implicit_return`, `question_mark_used`).

Test code is held to a lower standard — a broad `#![expect(...)]` at the top of a
test module is preferred over per-site suppressions scattered throughout.
