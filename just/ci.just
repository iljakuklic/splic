# Check formatting without modifying files.
check-fmt:
    cargo fmt --all --check

# Run Clippy lints.
clippy:
    cargo clippy --locked --workspace --all-targets

# Check rustdocs for broken links and warnings.
check-doc:
    RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps --all-features --document-private-items

# Run tests and check for snapshot drift.
test:
    cargo test --locked --workspace
    git diff --exit-code

# Run all CI checks.
all: check-fmt clippy check-doc test
