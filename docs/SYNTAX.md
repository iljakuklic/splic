# Syntax

Splic is a two-level language built on two-level type theory (2LTT). There is no syntactic distinction between type-level and term-level expressions.

## Design Principles

The high-level syntax takes inspiration from Rust: familiar, readable, with good ergonomics. The main differences from Rust are:

- **Added quotations and splices** for two-level types: `#(expr)` produces object-level code as a first-class meta-level value, while `$(expr)` embeds object-level code into meta-level context.
- **No syntactic separation** between type-level and term-level expressions to support dependent types, where the same expression syntax appears in both positions.

## Comments

Line comments start with `//` and extend to the end of the line:

```
// This is a comment
x + y  // This is also a comment
```

## Keywords

| Keyword   | Description |
|-----------|-------------|
| `fn`      | Function definition or function type |
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

## Function Types

Function types use the `fn` keyword with parenthesized parameters:

```
fn(_: u64) -> u64                // non-dependent function type (wildcard name required)
fn(x: u64) -> u64                // dependent: return type may mention x
fn(A: Type, x: A) -> A           // polymorphic: type parameter used in value positions
fn(_: fn(_: u64) -> u64) -> u64  // higher-order: function taking a function
```

Function types are right-associative: `fn(_: A) -> fn(_: B) -> C` means `fn(_: A) -> (fn(_: B) -> C)`.

Multi-parameter function types desugar to nested single-parameter types:

```
fn(A: Type, x: A) -> A   ≡   fn(A: Type) -> fn(x: A) -> A
```

Function types are meta-level only — they inhabit `Type`, not `VmType`.

## Lambda Expressions

Lambdas use Rust's closure syntax with mandatory type annotations:

```
|x: u64| x + 1                   // single parameter
|x: u64, y: u64| x + y           // multi-parameter (desugars to nested lambdas)
|f: fn(_: u64) -> u64, x: u64| f(x) // higher-order
```

Type annotations on lambda parameters are required. This makes lambdas inferable — the typechecker can synthesise the full function type from the annotations and the body.

Lambdas are meta-level only — they cannot appear in object-level (`code fn`) bodies.

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

Note: `|` as bitwise OR is distinguished from `|` as lambda delimiter by position: a leading `|` in atom position starts a lambda; `|` after an expression is bitwise OR.

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
             | fn_type                          -- function type
             | lambda                           -- lambda expression
             | "#(" expr ")"                    -- quotation
             | "#{" stmt* expr "}"              -- block quotation
             | "$(" expr ")"                    -- splice
             | "${" stmt* expr "}"              -- block splice
             | "[[" expr "]]"                   -- lifting
             | "match" expr "{" match_arm* "}"
             | block

fn_type     ::= "fn" "(" fn_params ")" "->" expr
fn_params   ::= (fn_param ("," fn_param)*)?
fn_param    ::= identifier ":" expr             -- name required; use "_" for non-dependent

lambda      ::= "|" param ("," param)* "|" expr

binary_op   ::= "+" | "-" | "*" | "/" | "==" | "!=" | "<" | ">" | "<=" | ">=" | "&" | "|"
unary_op    ::= "!"

literal     ::= "0" | "1" | "2" | ...
identifier  ::= [a-zA-Z_][a-zA-Z0-9_]*
```
