# Proposed Unified Definition Syntax

Proposal to unify the syntax for all definition-related constructs: global values and
functions, local values and functions, anonymous lambdas, and function types. See
[SYNTAX.md](../SYNTAX.md) for the currently implemented syntax.

## Design Decisions

- **`def`** for global named constructs — replaces `fn` for global function definitions;
  covers both constants and functions uniformly
- **`let`** for local named constructs — extended to support local function definitions,
  not just simple value bindings
- **`fn`** exclusively for function types (pi types) — never introduces a value
- **`lam`** for anonymous lambda expressions
- **`->`** for return type annotations in definitions and lambdas; **`:`** for type
  annotations on parameter-free bindings
- **`;` always required** after every definition body, including block bodies — moving
  between expression and block form only requires adding/removing braces, not adjusting
  punctuation
- **`def` allows self-reference** — recursive definitions are permitted; any restrictions
  (e.g. for zkVM termination) are enforced by semantic checks, not syntax
- **Object-level lambdas are not allowed** — `lam` is meta-level only, matching the
  existing restriction on lambdas

## Syntax

### Global definitions (`def`)

```
def x: u64 = 5;                         // global constant, explicit type
def x = 5;                              // global constant, inferred type (if permitted)
def f(x: u64) -> u64 = x + 1;          // global function, expression body
def f(x: u64) -> u64 = { x + 1 };      // global function, block body
def f(x: u64, y: u64) -> u64 = x + y;  // multi-parameter
code def f(x: u64) -> u64 = x + 1;     // object-level function
```

Type and return type annotations are required for global definitions (cannot be inferred
at the top level).

### Local definitions (`let`)

```
let x = 5;                              // local value, inferred type
let x: u64 = 5;                        // local value, explicit type
let f(x: u64) -> u64 = x + n;         // local function (may close over n)
let f(x: u64) -> u64 = { x + n };     // local function, block body
```

Type annotations are optional for locals.

### Lambdas (`lam`)

```
lam(x: u64) = x + 1                    // inferred return type
lam(x: u64) -> u64 = x + 1            // explicit return type
lam(x: u64, y: u64) = x + y           // multi-parameter
```

Lambdas are expressions, not statements — no trailing `;`. Parameter type annotations are
required (as currently), making the lambda's type fully synthesisable.

### Function types (`fn`)

Unchanged from current syntax:

```
fn(_: u64) -> u64                      // non-dependent
fn(x: u64) -> u64                      // dependent (return type may mention x)
fn(A: Type) -> fn(x: A) -> A          // polymorphic
```

## Progressive Enhancement

Each form is a small, mechanical addition to the previous:

| Form | Syntax |
|------|--------|
| Function type | `fn(x: u64) -> u64` |
| Lambda | `lam(x: u64) -> u64 = x + 1` |
| Local binding | `let f(x: u64) -> u64 = x + 1;` |
| Global binding | `def f(x: u64) -> u64 = x + 1;` |

Desugaring (meta-level only):
```
let f(x: u64) -> u64 = e;   ≡   let f: fn(x: u64) -> u64 = lam(x: u64) -> u64 = e;
def f(x: u64) -> u64 = e;   ≡   def f: fn(x: u64) -> u64 = lam(x: u64) -> u64 = e;
```

`code def` does not desugar to a lambda — the object-level sublanguage does not have
first-class functions.

## Grammar Changes

Proposed grammar (delta from [SYNTAX.md](../SYNTAX.md)):

```
top_stmt    ::= def_stmt

def_stmt    ::= ("code")? "def" identifier def_sig_req "=" expr ";"
let_stmt    ::= "let" identifier def_sig_opt "=" expr ";"

def_sig_req ::= "(" params ")" "->" expr      -- function: params + required return type
              | ":" expr                       -- value: required type annotation

def_sig_opt ::= ("(" params ")" ("->" expr)?)?  -- function: params + optional return type
              | (":" expr)?                      -- value: optional type annotation

lambda      ::= "lam" "(" params ")" ("->" expr)? "=" expr
```

Notable changes from current grammar:
- `fn_def` / `code_fn_def` replaced by `def_stmt`; body form changes from `block` to `"=" expr ";"`
- `let_stmt` extended with `def_sig` to support local function definitions
- `lambda` syntax changes from `"|" params "|" expr` to `"lam" "(" params ")" ("->" expr)? "=" expr`;
  `|` is freed from lambda duty and the operator table ambiguity note in SYNTAX.md can be removed
  (resolves [#23](https://github.com/iljakuklic/splic/issues/23))

## Open Questions

### Default / implicit parameter syntax

How to express optional or tactic-filled parameters. Candidate syntax uses `?=` as a
distinct default operator:

```
def f(x: u64, n: u64 ?= 0) -> u64 = x + n;
def id(A: Type ?= ?(auto), x: A) -> A = x;
```

`?=` pairs naturally with `?(tactic)` — a general hole/elaboration-request expression
valid anywhere an expression is permitted. `?` should be reserved as a prefix sigil for
this purpose (not used for optionals, error propagation, etc.).

Deferred — the current proposals do not prevent this from being added.

## Incremental Implementation

These changes are independent enough to land separately:

1. **`lam` keyword**: Replace `|params| expr` with `lam(params) (-> T)? = expr` — purely
   syntactic, no new semantics
2. **`let` with parameters**: Allow `let f(params) -> T = expr;` for local function
   definitions — syntactic sugar over existing local bindings
3. **`def` for functions**: Replace global `fn name(params)` with `def name(params)`;
   change body syntax from `{ ... }` to `= expr;` — syntactic change to existing
   functionality
4. **`def` for constants**: Allow parameter-free `def x: T = expr;` at the top level —
   requires new semantics (top-level constants do not currently exist)
5. **Default parameters**: Add `?=` once the design is finalised
