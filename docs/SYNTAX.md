# Syntax

Splic is a two-level language built on two-level type theory (2LTT).

## Keywords

| Keyword   | Description |
|-----------|-------------|
| `fn`      | Function definition |
| `code`    | Object-level marker |
| `let`     | Variable binding |
| `match`   | Pattern matching |

## Builtins

| Builtin   | Description |
|-----------|-------------|
| `u0`      | Unit type |
| `u1`      | Boolean type |
| `u8`      | 8-bit unsigned |
| `u16`     | 16-bit unsigned |
| `u32`     | 32-bit unsigned |
| `u64`     | 64-bit unsigned |
| `Type`    | Meta-level universe |
| `VmType`  | Object-level universe |
| `[[T]]`   | Lifting (code type) |

Identifiers matching `u[0-9]+` are reserved for primitive types.

## Operators

### Arithmetic
`+` `-` `*` `/`

### Comparison
`==` `!=` `<` `>` `<=` `>=`

Note: These operators are provisional. See [bs/comparison_operators.md] for discussion.

### Bitwise / Logical
`!` `&` `|`

## Grammar (EBNF-like)

```
program     ::= top_stmt*

top_stmt    ::= fn_def
             | code_fn_def

fn_def      ::= "fn" identifier "(" params ")" "->" type block
code_fn_def ::= "code" fn_def

params      ::= (param ("," param)*)?
param       ::= identifier ":" type

type        ::= primitive_type
             | "[[" expr "]]"
             | "(" type ("," type)* ")"

block       ::= "{" stmt* expr "}"
stmt        ::= "let" identifier "=" expr ";"
             | "match" expr "{" match_arm* "}"
             | expr ";"

match_arm   ::= literal "=>" expr ","
             | identifier "=>" expr ","
             | "_" "=>" expr ","

expr        ::= literal
             | identifier
             | expr "(" expr ("," expr)* ")"
             | expr binary_op expr
             | unary_op expr
             | "#(" expr ")"
             | "#{" stmt* expr "}"
             | "$(" expr ")"
             | "${" stmt* expr "}"
             | "let" identifier "=" expr "in" expr
             | "match" expr "{" match_arm* "}"
             | "(" expr ("," expr)* ")"
             | block

binary_op   ::= "+" | "-" | "*" | "/" | "==" | "!=" | "<" | ">" | "<=" | ">=" | "&" | "|"
unary_op    ::= "!"

literal     ::= "0" | "1" | "2" | ...
identifier  ::= [a-zA-Z_][a-zA-Z0-9_]*
```
