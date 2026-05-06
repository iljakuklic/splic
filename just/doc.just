[private]
default:
    just --list doc

[private]
require-doc-md:
    @command -v cargo-doc-md > /dev/null 2>&1 || { echo "cargo-doc-md is not installed. Run: cargo install cargo-doc-md"; exit 1; }

# Generate Markdown docs for specific crates, e.g. `just doc::md -p wasm-encoder` (requires cargo-doc-md).
md +args: require-doc-md
    cargo doc-md --no-deps {{args}}

# Generate Markdown docs for the entire workspace into target/doc-md/ (requires cargo-doc-md).
md-full: require-doc-md
    cargo doc-md --workspace --include-private

# Generate HTML docs for the entire workspace into target/doc/ (all features, private items).
html-full:
    cargo doc --workspace --all-features --document-private-items

