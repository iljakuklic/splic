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
    cargo clippy --locked --workspace --all-targets -- \
        -D warnings \
        -D clippy::use_self \
        -D clippy::derive_partial_eq_without_eq \
        -D clippy::uninlined_format_args \
        -D clippy::elidable_lifetime_names \
        -D clippy::doc_markdown \
        -D clippy::match_same_arms \
        -D clippy::unnecessary_wraps \
        -D clippy::used_underscore_binding \
        -D clippy::map_unwrap_or \
        -D clippy::redundant_closure_for_method_calls \
        -D clippy::return_self_not_must_use \
        -D clippy::redundant_test_prefix \
        -D clippy::unused_trait_names \
        -D clippy::missing_const_for_fn \
        -D clippy::trivially_copy_pass_by_ref \
        -D clippy::cast_possible_truncation \
        -D clippy::explicit_iter_loop \
        -D clippy::wildcard_enum_match_arm \
        -D clippy::indexing_slicing

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
