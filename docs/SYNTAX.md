# Syntax

Splic is a two-level language built on two-level type theory (2LTT). There is no syntactic distinction between type-level and term-level expressions.

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
| `[[e]]`   | Lifting (code type), corresponds to ⇑ in 2LTT literature |
| `#(e)`    | Quote, corresponds to ⟨⟩ in 2LTT literature |
| `$(e)`    | Splice, corresponds to ∼ in 2LTT literature |

Identifiers matching `u[0-9]+` are reserved for primitive types.

## Operators

Lowest to highest, left-associative unless noted:

| Precedence | Operators |
|------------|-----------|
| 1 | `\|` |
| 2 | `&` |
| 3 | `==` `!=` `<` `>` `<=` `>=` |
| 4 | `+` `-` |
| 5 | `*` `/` |
| 6 | `!` (unary) |

Note: The comparison operators are provisional. See [bs/comparison_operators.md](bs/comparison_operators.md) for discussion.

## Grammar (EBNF-like)

```
program     ::= top_stmt*

top_stmt    ::= fn_def
             | code_fn_def

fn_def      ::= "fn" identifier "(" params ")" "->" expr block
code_fn_def ::= "code" fn_def

params      ::= (param ("," param)*)?
param       ::= identifier ":" expr

block       ::= "{" stmt* expr "}"   -- returns value of expr
stmt        ::= "let" identifier (":" expr)? "=" expr ";"

match_arm   ::= pattern "=>" expr ","

pattern     ::= literal
             | identifier    -- note: "_" is parsed as an identifier too

expr        ::= literal
             | identifier
             | expr "(" expr ("," expr)* ")"   -- application
             | expr binary_op expr
             | unary_op expr
             | "#(" expr ")"                   -- quotation
             | "#{" stmt* expr "}"              -- block quotation
             | "$(" expr ")"                    -- splice
             | "${" stmt* expr "}"              -- block splice
             | "[[" expr "]]"                   -- lifting
             | "match" expr "{" match_arm* "}"
             | block

binary_op   ::= "+" | "-" | "*" | "/" | "==" | "!=" | "<" | ">" | "<=" | ">=" | "&" | "|"
unary_op    ::= "!"

literal     ::= "0" | "1" | "2" | ...
identifier  ::= [a-zA-Z_][a-zA-Z0-9_]*
```
