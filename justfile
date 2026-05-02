# CI checks (fmt, lint, doc, test).
mod ci 'just/ci.just'

# Documentation generation and checking.
mod doc 'just/doc.just'

# Fuzz testing via bolero.
mod fuzz 'just/fuzz.just'

# Run under Miri or sanitizers.
mod sanitizers 'just/sanitizers.just'

# `default` must remain at the top.
[private]
default:
    just --list

# Format the codebase.
fmt:
    cargo fmt --all

# Run Clippy lints.
clippy: ci::clippy

# Apply Clippy auto-fixes.
clippy-fix:
    cargo clippy --locked --workspace --all-targets --fix --allow-dirty

# Run tests.
test:
    cargo test --locked --workspace

# Regenerate expect-test snapshots.
update-snapshots:
    UPDATE_EXPECT=1 cargo test --locked --workspace

# Stage a Splic source file, printing the object-level program.
stage *args:
    cargo run -- stage {{args}}

# Compile a Splic source file to WebAssembly.
compile *args:
    cargo run -- compile --target wasm {{args}}
