# AGENTS.md

## Project Overview

Splic is a Rust-based experimental programming language targeting zkVMs, built on two-level type theory (2LTT). The project uses a workspace structure with a main compiler crate.

## Project Structure

```
splic/
├── compiler/          # Main compiler crate
├── docs/              # Documentation
├── Cargo.toml         # Workspace configuration
└── Cargo.lock         # Dependency lock file
```

## Development Commands

All common workflow tasks are available via `just`. Run `just` to list recipes.

### CI checks (use these before committing)
```bash
just ci          # Full CI: fmt check + clippy + tests (mirrors CI exactly)
just check-fmt   # Check formatting without modifying files
just clippy      # Run Clippy with the full lint set
just clippy-fix  # Apply Clippy auto-fixes
just fmt         # Format the codebase
```

### Testing
```bash
cargo test                          # Run all tests
cargo test -p splic-compiler        # Run tests for the compiler crate only
cargo test -p splic-compiler <FILTER>  # Run matching tests
```

Note: `just test` adds `--locked` and snapshot drift detection; prefer `cargo test` directly during development for flexibility.

### Checking / building
```bash
cargo check --workspace --all-targets   # Fast syntax + type check (includes test code)
cargo build                             # Build all workspace members
```

### Staging a metaprogram
```bash
cargo run -- stage <FILE>
```

Stages a Splic source file, printing the object-level code with all meta-level computation resolved.

### Fuzzing
```bash
cargo bolero test           # Run bolero fuzz tests
```

## Testing & Quality

### Test Structure
- Tests located in `compiler/src/test/`
- Uses **rstest** for parameterized tests
- Snapshot testing with **expect-test** (diff output may show ANSI color codes which can be misleading - if colors appear in the diff, run `UPDATE_EXPECT=1 cargo test` to regenerate snapshots and verify actual state)
- Fuzz tests in `fuzz.rs`
- Note: When adding new `.input.txt` test files, run `cargo clean -p splic-compiler` first to ensure they're picked up by the test framework

### Clippy
The project enforces a curated set of lints beyond Clippy defaults — see `just clippy` in the justfile for the full list. All lints are `-D` (hard errors). For test modules it is acceptable to suppress noisy lints with a broad `#![allow(...)]` at the top of the file rather than per-site annotations.

## Coding Guidelines

### Commit Messages
- Use [Conventional Commits](https://www.conventionalcommits.org/) prefixes: `feat`, `fix`, `refactor`, `docs`, `test`, `chore`, etc.
- Use `ai:` for changes to AI agent scaffolding: `AGENTS.md`, skills, opencode config, etc.

### Formatting
- Do not reorder existing `use` items; rely on `cargo fmt` to handle import ordering

### Error Handling
- Use `anyhow` for error handling
- Default features disabled for minimal dependencies

### Memory Management
- Use `bumpalo` arena allocator wherever practical
- For arena-allocated structures, refer to other objects using plain references rather than `Box`

### 2LTT Patterns
- No syntactic separation between type-level and term-level expressions
- Quotations (`#(e)`, `#{...}`) and splices (`$(e)`, `${...}`) for metaprogramming
- Lifting with `[[e]]`

## Language Design

Splic is built on **two-level type theory (2LTT)**:
- **Meta-level**: Purely functional dependently typed language
- **Object-level**: Low-level language for zkvm bytecode
- Connected through quotations and splices for type-safe metaprogramming

See `docs/CONCEPT.md` and `docs/SYNTAX.md` for detailed language specifications.
