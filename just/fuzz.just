[private]
default:
    just --list fuzz

bolero := "cargo +nightly bolero test -p splic-compiler"

# Fuzz a target by its full test path.
run target timeout="60s" jobs="1":
    mkdir -p "{{justfile_directory()}}/target/fuzz-{{replace(target, "::", "-")}}"
    cd "{{justfile_directory()}}/target/fuzz-{{replace(target, "::", "-")}}" && {{bolero}} -T {{timeout}} -j {{jobs}} {{target}}

# Fuzz the lexer against arbitrary strings.
lexer-lexer timeout="60s" jobs="1": (run "lexer::test::fuzz::lexer" timeout jobs)

# Fuzz the lexer against a single token.
lexer-token timeout="60s" jobs="1": (run "lexer::test::fuzz::token" timeout jobs)

# Fuzz the parser's expression entrypoint.
parser-expr timeout="60s" jobs="1": (run "parser::test::fuzz_parse_expr" timeout jobs)

# Fuzz the parser's program entrypoint.
parser-program timeout="60s" jobs="1": (run "parser::test::fuzz_parse_program" timeout jobs)

# Fuzz all four targets in parallel (2 threads for lexer, 4 for parser).
[parallel]
all-parallel timeout="60s": \
    (lexer-lexer timeout "2") \
    (lexer-token timeout "2") \
    (parser-expr timeout "4") \
    (parser-program timeout "4")
