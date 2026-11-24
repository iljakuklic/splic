# Run all CI checks.
all: qa check-clean

# Run all CI checks on the working copy (uncommitted changes allowed).
qa: check-fmt clippy check-doc test

# Check formatting without modifying files.
check-fmt:
    cargo fmt --all --check

# Run Clippy lints.
clippy:
    cargo clippy --locked --workspace --all-targets

# Check rustdocs for broken links and warnings.
check-doc:
    RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps --all-features --document-private-items

# Run tests.
test:
    cargo test --locked --workspace

# Check that the working tree is clean (catches snapshot drift).
check-clean:
    git diff --exit-code
