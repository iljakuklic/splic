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

### Verifying syntax and type correctness
```bash
cargo check -p splic-compiler --all-targets   # Check a specific crate
cargo check --workspace --all-targets   # Check whole workspace
```

The `--all-targets` switch also includes the test code.

### Building
```bash
cargo build                     # Build all workspace members
cargo build -p splic-compiler   # Build specific crate
```

### Testing
```bash
cargo test                     # Run all tests
cargo test -p splic-compiler   # Run tests for specific crate
cargo test --workspace   # Run tests for all crates crate
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

### Quality Tools
- Uses clippy defaults (no explicit config)
- Rust 2024 edition
- Minimal dependencies with `default-features = false`

## Coding Guidelines

### Formatting
- Use inline style formatters in error messages: `format!("expected {expected}, got {token:?}")` instead of `format!("expected {}, got {:?}", expected, token)`
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

### Workspace Commands
- Always use `-p splic-compiler` when targeting the compiler crate
- Example: `cargo test -p splic-compiler`
- Use `--workspace` when targeting all the crates

## Language Design

Splic is built on **two-level type theory (2LTT)**:
- **Meta-level**: Purely functional dependently typed language
- **Object-level**: Low-level language for zkvm bytecode
- Connected through quotations and splices for type-safe metaprogramming

See `docs/CONCEPT.md` and `docs/SYNTAX.md` for detailed language specifications.
