# Run under Miri to detect undefined behavior and memory leaks.
miri:
    MIRIFLAGS="-Zmiri-disable-isolation" cargo +nightly miri test --quiet -p splic-compiler -- --test-threads=1

# Run under LeakSanitizer to detect memory leaks.
lsan:
    RUSTFLAGS="-Zsanitizer=leak" cargo +nightly test -Zbuild-std --target x86_64-unknown-linux-gnu -p splic-compiler
