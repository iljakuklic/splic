[private]
default:
    just --list fuzz

bolero := "cargo +nightly bolero test -p splic-compiler"

# Fuzz the lexer against arbitrary strings.
lexer-lexer timeout="60s" jobs="1":
    {{bolero}} -T {{timeout}} -j {{jobs}} lexer::test::fuzz::lexer

# Fuzz the lexer against a single token.
lexer-token timeout="60s" jobs="1":
    {{bolero}} -T {{timeout}} -j {{jobs}} lexer::test::fuzz::token

# Fuzz the parser's expression entrypoint.
parser-expr timeout="60s" jobs="1":
    {{bolero}} -T {{timeout}} -j {{jobs}} parser::test::fuzz_parse_expr

# Fuzz the parser's program entrypoint.
parser-program timeout="60s" jobs="1":
    {{bolero}} -T {{timeout}} -j {{jobs}} parser::test::fuzz_parse_program

# Fuzz all four targets in parallel (2 threads for lexer, 4 for parser).
[parallel]
all-parallel timeout="60s": \
    (lexer-lexer timeout "2") \
    (lexer-token timeout "2") \
    (parser-expr timeout "4") \
    (parser-program timeout "4")
