# AGENTS.md

## Project Overview

Splic is a Rust-based experimental programming language targeting zkVMs, built on two-level type theory (2LTT). The project uses a workspace structure with a main compiler crate.

## Project Structure

```
splic/
├── compiler/          # Main compiler library crate
│   └── src/
│       ├── lexer/     # Tokenization
│       ├── parser/    # Parsing (string → AST)
│       ├── checker/   # Type checking, elaboration, dependent types (uses NbE)
│       ├── core/      # Core language abstractions
│       ├── staging/   # Meta-level staging (NbE-based code generation)
│       └── common/    # Shared utilities
├── cli/               # CLI binary crate (depends on compiler)
├── docs/              # Documentation
│   ├── README.md      # Language design and user-facing docs
│   └── bs/            # Implementation notes and proposals
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
cargo test --quiet                  # Run all tests (reduced output)
cargo test -p splic-compiler        # Run tests for the compiler crate only
cargo test -p splic-compiler <FILTER>  # Run matching tests
```

Use `--quiet` to suppress per-test output and only show the summary lines; errors still appear in full.

Note: `just test` adds `--locked`; `just test-full` additionally checks for snapshot drift and is used in CI. Prefer `cargo test` directly during development for flexibility.

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
- Unit tests located throughout `compiler/src/` in `test` modules (e.g., `compiler/src/lexer/test/`, `compiler/src/parser/test/`)
- Integration tests in `compiler/tests/`
- Uses **rstest** for parameterized tests
- Snapshot testing with **expect-test** (diff output may show ANSI color codes which can be misleading - if colors appear in the diff, run `UPDATE_EXPECT=1 cargo test` to regenerate snapshots and verify actual state)
- Fuzz tests with **bolero** in component `test` modules
- Note: When adding new `.input.txt` test files, run `cargo clean -p splic-compiler` first to ensure they're picked up by the test framework

### Clippy
The project enforces a curated set of lints beyond Clippy defaults — see `[workspace.lints]` in `Cargo.toml` for the full list. All lints are `"deny"`. Use `#[expect(...)]` to suppress a lint at a specific site. For test modules it is acceptable to use a broad `#![expect(...)]` at the top of the file rather than per-site annotations.

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
- In NbE (semantic evaluation), use slices `&'a [Value<'a>]` for environment snapshots captured in closures, not vectors

### 2LTT Patterns
- No syntactic separation between type-level and term-level expressions
- Quotations (`#(e)`, `#{...}`) and splices (`$(e)`, `${...}`) for metaprogramming
- Lifting with `[[e]]`

## Documentation

Splic documentation is organized in two main locations:

- **`docs/README.md`** — Overview and index of language design and user-facing docs (CONCEPT, SYNTAX, examples)
- **`docs/bs/README.md`** — Index of implementation notes, proposals, and architecture documentation

**Guidelines for writing docs:**
- Focus on architectural concepts and design decisions ("what" and "why") rather than implementation-specific details (function names, parameter types, exact APIs). This keeps docs resilient to code changes.
- Keep doc indices up to date: when adding new files, add entries to the appropriate `README.md` with a brief description.

## Language Design

Splic is built on **two-level type theory (2LTT)**:
- **Meta-level**: Purely functional dependently typed language
- **Object-level**: Low-level language for zkvm bytecode
- Connected through quotations and splices for type-safe metaprogramming
- See the 2ltt skill for more detail
