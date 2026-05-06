[private]
default:
    just --list fuzz

bolero := "cargo +nightly bolero test -p splic-compiler"

# Fuzz the lexer against arbitrary strings.
lexer-lexer timeout="60s":
    {{bolero}} -T {{timeout}} lexer::test::fuzz::lexer

# Fuzz the lexer against a single token.
lexer-token timeout="60s":
    {{bolero}} -T {{timeout}} lexer::test::fuzz::token

# Fuzz the parser's expression entrypoint.
parser-expr timeout="60s":
    {{bolero}} -T {{timeout}} parser::test::fuzz_parse_expr

# Fuzz the parser's program entrypoint.
parser-program timeout="60s":
    {{bolero}} -T {{timeout}} parser::test::fuzz_parse_program
