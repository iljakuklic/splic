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

# Run tests and check for snapshot drift.
test:
    cargo test --locked --workspace
    git diff --exit-code

# Run tests under Miri to detect undefined behavior and memory leaks.
miri:
    MIRIFLAGS="-Zmiri-disable-isolation" cargo +nightly miri test --quiet -p splic-compiler -- --test-threads=1

# Run tests under LeakSanitizer to detect memory leaks.
lsan:
    RUSTFLAGS="-Zsanitizer=leak" cargo +nightly test -Zbuild-std --target x86_64-unknown-linux-gnu -p splic-compiler

# Run all CI checks.
ci: check-fmt clippy test
