# Syntax

Splic is a two-level language built on two-level type theory (2LTT). There is no syntactic distinction between type-level and term-level expressions.

## Design Principles

- **Orthogonality**: The basic building blocks are independent and suggestive of their semantics. Combining them should produce predictable results.
- **Progressive enhancement**: Related syntactic concepts (function definitions, lambdas, function types) look similar and it is easy to move between them.
- **Aesthetics**: The syntax should be pleasant to read. Rust is a good starting point.
- **Explicit**: No hidden magic. Syntax sugar has a straightforward desugaring to more basic constructs. Annotations are available wherever they are useful, even if not always required.
- **Uniformity**: The same construct works the same way everywhere. No special cases for specific positions or contexts.
- **Tooling-friendly**: The grammar should be unambiguous and easy to parse, supporting the compiler and tools like formatters, syntax highlighters, and language servers without heroics.
- **Unlimited weirdness budget**: While Rust is a starting point, we are not afraid to deviate if a different choice better serves the principles above.

## Comments

Line comments start with `//` and extend to the end of the line:

```
// This is a comment
x + y  // This is also a comment
```

## Keywords

| Keyword   | Description |
|-----------|-------------|
| `def`     | Global definition (function or constant) |
| `let`     | Local variable binding |
| `lam`     | Anonymous lambda expression |
| `fn`      | Function type (pi type) |
| `code`    | Object-level marker |
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

## Global Definitions

Global definitions use `def` with a required return type annotation and `= expr;` body:

```
def f(x: u64) -> u64 = x + 1;          // function, expression body
def f(x: u64) -> u64 = { x + 1 };      // function, block body
def f(x: u64, y: u64) -> u64 = x + y;  // multi-parameter
code def f(x: u64) -> u64 = x + 1;     // object-level function
```

`code def` marks object-level functions. Type and return type annotations are required
(cannot be inferred at the top level). `def` allows self-reference — recursion is permitted.

`def f(params) -> T = e;` is syntactic sugar for `def f: fn(params) -> T = lam(params) -> T = e;`.
This desugaring applies at the meta level only; `code def` does not desugar to a lambda —
the object-level sublanguage does not have first-class functions.

Multiple parameter groups on `def` are supported for curried definitions (see
[Curried Parameter Groups](#curried-parameter-groups) below). `code def` does not support
multiple parameter groups.

## Local Bindings

Local bindings use `let` inside a block. Type annotations are optional:

```
let x = 5;                              // value, inferred type
let x: u64 = 5;                        // value, explicit type
let f(x: u64) = x + 1;                // local function, inferred return type
let f(x: u64) -> u64 = x + 1;        // local function, explicit return type
let f(x: u64) -> u64 = { x + n };    // local function, block body (may close over n)
```

`let f(params) (-> T)? = e;` is syntactic sugar for `let f (: fn(params) -> T)? = lam(params) (-> T)? = e;`.

Multiple parameter groups on `let` are supported for curried local functions (see
[Curried Parameter Groups](#curried-parameter-groups) below).

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

Multiple parameter groups `fn(p1)(p2) -> T` are also supported (see
[Curried Parameter Groups](#curried-parameter-groups) below).

## Lambda Expressions

Lambdas use the `lam` keyword with mandatory parameter type annotations:

```
lam(x: u64) = x + 1                       // single parameter
lam(x: u64, y: u64) = x + y               // multi-parameter
lam(f: fn(_: u64) -> u64, x: u64) = f(x)  // higher-order
lam(x: u64) -> u64 = x + 1                // with explicit return type
lam() = expr                               // nullary: produces a fn() -> T value
```

Type annotations on lambda parameters are required. This makes lambdas inferable — the
typechecker can synthesise the full function type from the annotations and the body. An
optional `-> T` return type annotation is also supported.

Lambdas are meta-level only — they cannot appear in object-level (`code def`) bodies.

Multiple parameter groups `lam(p1)(p2) = e` are also supported (see
[Curried Parameter Groups](#curried-parameter-groups) below).

## Curried Parameter Groups

All four constructs (`def`, `let`, `fn`, `lam`) accept multiple parenthesised parameter
groups. Each group becomes one level of lambda/pi nesting, enabling dependent currying:

```
// Equivalent ways to write a curried polymorphic identity:
def id(A: Type)(x: A) -> A = x;
def id: fn(A: Type)(x: A) -> A = lam(A: Type)(x: A) = x;
def id: fn(A: Type) -> fn(x: A) -> A = lam(A: Type) = lam(x: A) = x;
```

The `-> T` return type annotation always applies to the innermost body. All parameters
from all groups are in scope for the return type and body, enabling dependent currying:

```
def const(A: Type)(B: Type) -> fn(_: A) -> fn(_: B) -> A =
    lam(x: A)(y: B) = x;
```

**Desugaring rules:**

```
lam(p1)(p2)...(pN) (-> T)? = e
  ≡  lam(p1) = lam(p2) = ... = lam(pN) (-> T)? = e

let f(p1)(p2)...(pN) (-> T)? = e;
  ≡  let f = lam(p1)(p2)...(pN) (-> T)? = e;

def f(p1)(p2)...(pN) -> T = e;
  ≡  def f: fn(p1)(p2)...(pN) -> T = lam(p1)(p2)...(pN) -> T = e;

fn(p1)(p2)...(pN) -> T
  ≡  fn(p1) -> fn(p2) -> ... -> fn(pN) -> T
```

Note: within a parameter group, parameters are still declared tuple-style
(comma-separated), so `lam(x: u64, y: u64)` and `lam(x: u64)(y: u64)` are distinct:
the former produces a single two-argument lambda; the latter produces two nested
single-argument lambdas.

`code def` does not support multiple parameter groups — object-level functions always
have exactly one parameter group.

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

top_stmt    ::= def_stmt

def_stmt    ::= ("code")? "def" identifier def_sig_req "=" expr ";"

def_sig_req ::= param_groups "->" expr   -- function: one or more param groups + required return type
              | ":" expr                 -- value: required type annotation

param_groups ::= ("(" params ")")+       -- one or more parameter groups

params      ::= (param ("," param)*)?
param       ::= identifier ":" expr

block       ::= "{" stmt* expr "}"   -- returns value of expr
stmt        ::= let_stmt

let_stmt    ::= "let" identifier def_sig_opt "=" expr ";"

def_sig_opt ::= param_groups ("->" expr)?   -- function: one or more groups, optional return type
              | (":" expr)?                 -- value: optional type annotation

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

fn_type     ::= "fn" param_groups "->" expr
fn_params   ::= (fn_param ("," fn_param)*)?
fn_param    ::= identifier ":" expr             -- name required; use "_" for non-dependent

lambda      ::= "lam" param_groups ("->" expr)? "=" expr

binary_op   ::= "+" | "-" | "*" | "/" | "==" | "!=" | "<" | ">" | "<=" | ">=" | "&" | "|"
unary_op    ::= "!"

literal     ::= "0" | "1" | "2" | ...
identifier  ::= [a-zA-Z_][a-zA-Z0-9_]*
```
