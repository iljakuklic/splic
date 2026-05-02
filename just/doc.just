# Check that cargo-doc-md is installed.
_require-doc-md:
    @command -v cargo-doc-md > /dev/null 2>&1 || { echo "cargo-doc-md is not installed. Run: cargo install cargo-doc-md"; exit 1; }

# Generate Markdown docs for one or more dependencies (requires cargo-doc-md).
# Usage: just doc::md -p wasm-encoder
# Usage: just doc::md -p wasm-encoder -p wasmparser
# Usage: just doc::md -p wasm-encoder --include-private
md +args: _require-doc-md
    cargo doc-md --no-deps {{args}}

# Generate full Markdown docs for the entire workspace and all dependencies (private items).
# Note: cargo-doc-md does not support --all-features; features reflect workspace defaults.
# Output in target/doc-md/.
md-full: _require-doc-md
    cargo doc-md --workspace --include-private

# Generate full HTML docs for the entire workspace (all features, private items).
# Output in target/doc/.
html-full:
    cargo doc --workspace --all-features --document-private-items

# Check rustdocs for broken links and warnings (used in CI).
check:
    RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps --all-features --document-private-items
