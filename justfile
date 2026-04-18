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
    cargo clean -p splic-compiler -p splic-driver
    UPDATE_EXPECT=1 cargo test --locked --workspace

bolero := "cargo +nightly bolero test -p splic-compiler"

# Fuzz the lexer against arbitrary strings.
fuzz-lexer-lexer timeout="60s":
    {{bolero}} -T {{timeout}} lexer::test::fuzz::lexer

# Fuzz the lexer against a single token.
fuzz-lexer-token timeout="60s":
    {{bolero}} -T {{timeout}} lexer::test::fuzz::token

# Fuzz the parser's expression entrypoint.
fuzz-parser-expr timeout="60s":
    {{bolero}} -T {{timeout}} parser::test::fuzz_parse_expr

# Fuzz the parser's program entrypoint.
fuzz-parser-program timeout="60s":
    {{bolero}} -T {{timeout}} parser::test::fuzz_parse_program

# Run under Miri to detect undefined behavior and memory leaks.
miri:
    MIRIFLAGS="-Zmiri-disable-isolation" cargo +nightly miri test --quiet -p splic-compiler -- --test-threads=1

# Run under LeakSanitizer to detect memory leaks.
lsan:
    RUSTFLAGS="-Zsanitizer=leak" cargo +nightly test -Zbuild-std --target x86_64-unknown-linux-gnu -p splic-compiler

# Check that cargo-doc-md is installed.
_require-doc-md:
    @command -v cargo-doc-md > /dev/null 2>&1 || { echo "cargo-doc-md is not installed. Run: cargo install cargo-doc-md"; exit 1; }

# Generate Markdown docs for one or more dependencies (requires cargo-doc-md).
# Usage: just crate-docs -p wasm-encoder
# Usage: just crate-docs -p wasm-encoder -p wasmparser
# Usage: just crate-docs -p wasm-encoder --include-private
crate-docs +args: _require-doc-md
    cargo doc-md --no-deps {{args}}

# Generate full HTML docs for the entire workspace and all dependencies (all features, private items).
# Output in target/doc/.
doc-full:
    cargo doc --workspace --all-features --document-private-items

# Generate full Markdown docs for the entire workspace and all dependencies (private items).
# Note: cargo-doc-md does not support --all-features; features reflect workspace defaults.
# Output in target/doc-md/.
doc-md-full: _require-doc-md
    cargo doc-md --workspace --include-private

# Check rustdocs for broken links and warnings (used in CI).
check-doc:
    RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps --all-features --document-private-items

# Run all CI checks.
ci: check-fmt clippy check-doc test
