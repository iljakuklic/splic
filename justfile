fmt:
    cargo fmt --all

check-fmt:
    cargo fmt --all --check

clippy:
    cargo clippy --workspace --all-targets -- -D warnings

test:
    cargo test --workspace
    git diff --exit-code

ci: check-fmt clippy test
