mod doc 'just/doc.just'
mod fuzz 'just/fuzz.just'
mod sanitizers 'just/sanitizers.just'

# `default` must remain at the top.
# List all available recipes.
default:
    just --list

# Format the codebase.
fmt:
    cargo fmt --all

# Check formatting without modifying files (used in CI).
check-fmt:
    cargo fmt --all --check

# Run Clippy lints.
clippy:
    cargo clippy --locked --workspace --all-targets

# Apply Clippy auto-fixes.
clippy-fix:
    cargo clippy --locked --workspace --all-targets --fix --allow-dirty

# Run tests.
test:
    cargo test --locked --workspace

# Run tests and check for snapshot drift (used in CI).
test-full: test
    git diff --exit-code

# Regenerate expect-test snapshots.
update-snapshots:
    UPDATE_EXPECT=1 cargo test --locked --workspace

# Stage a Splic source file, printing the object-level program.
stage *args:
    cargo run -- stage {{args}}

# Compile a Splic source file to WebAssembly.
compile *args:
    cargo run -- compile --target wasm {{args}}

# Run all CI checks.
ci: check-fmt clippy doc::check test
