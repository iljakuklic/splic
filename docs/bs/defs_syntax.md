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

The `def`, `let`, and `lam` forms described in this proposal are now implemented — see
[SYNTAX.md](../SYNTAX.md) for the canonical reference. Brief summary with key examples:

**`def`** — global definitions with `= expr;` body; `code def` for object-level:
```
def f(x: u64, y: u64) -> u64 = x + y;
code def g(x: u64) -> u64 = x + 1;
```

**`let`** — local bindings with optional params and return type:
```
let f(x: u64) -> u64 = x + n;   // closes over n
```

**`lam`** — anonymous lambda with mandatory parameter annotations:
```
lam(x: u64, y: u64) = x + y
lam(x: u64) -> u64 = x + 1     // with explicit return type
```

**`fn`** — function types (pi types), unchanged:
```
fn(A: Type, x: A) -> A   ≡   fn(A: Type) -> fn(x: A) -> A
```

### Curried parameter groups (proposed)

Multiple parenthesised parameter groups are syntactic sugar for nested lambdas and
function types. The `-> T` return-type annotation, when present, applies to the
innermost body. All parameters from all groups are in scope for the return type and body,
enabling dependent currying:

```
// All equivalent ways to write a curried polymorphic identity:
def id(A: Type)(x: A) -> A = x;
def id: fn(A: Type)(x: A) -> A = lam(A: Type)(x: A) -> A = x;
def id: fn(A: Type) -> fn(x: A) -> A = lam(A: Type) = lam(x: A) = x;
```

Desugaring rules:

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

Note: within a parameter group, parameters are still declared tuple-style (comma-separated),
so `lam(x: u64, y: u64)` and `lam(x: u64)(y: u64)` are distinct:
the former produces a single two-argument lambda; the latter produces two nested
single-argument lambdas.

## Progressive Enhancement

Each form is a small, mechanical addition to the previous:

| Form | Syntax |
|------|--------|
| Function type | `fn(x: u64) -> u64` |
| Lambda | `lam(x: u64) -> u64 = x + 1` |
| Local binding | `let f(x: u64) -> u64 = x + 1;` |
| Global binding | `def f(x: u64) -> u64 = x + 1;` |
| Curried (any of the above) | `fn(A: Type)(x: A) -> A` / `lam(A: Type)(x: A) = x` / … |

Desugaring (meta-level only):
```
let f(x: u64) -> u64 = e;   ≡   let f: fn(x: u64) -> u64 = lam(x: u64) -> u64 = e;
def f(x: u64) -> u64 = e;   ≡   def f: fn(x: u64) -> u64 = lam(x: u64) -> u64 = e;
```

Curried desugaring (see [Curried parameter groups](#curried-parameter-groups-proposed)):
```
lam(A: Type)(x: A) = x   ≡   lam(A: Type) = lam(x: A) = x
fn(A: Type)(x: A) -> A   ≡   fn(A: Type) -> fn(x: A) -> A
```

`code def` does not desugar to a lambda — the object-level sublanguage does not have
first-class functions.

## Grammar Changes

The `def`, `let`, and `lam` grammar changes are implemented — see [SYNTAX.md](../SYNTAX.md).
The proposed curried extension requires the following additional changes:

```
param_groups ::= ("(" params ")")+             -- one or more parameter groups (currently exactly one)

def_sig_req ::= param_groups "->" expr         -- currently: "(" params ")" "->" expr
def_sig_opt ::= param_groups ("->" expr)?      -- currently: "(" params ")" ("->" expr)?
lambda      ::= "lam" param_groups ("->" expr)? "=" expr   -- currently: single "(" params ")"
fn_ty       ::= "fn" param_groups "->" expr    -- currently: single "(" fn_params ")"
              | expr "->" expr                  -- shorthand non-dependent (right-associative)
```

In each case the change is mechanical: replace a single `"(" params ")"` with `param_groups`
(one or more groups). No new tokens or precedence rules are needed.

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
   syntactic, no new semantics ✅
2. **`let` with parameters**: Allow `let f(params) -> T = expr;` for local function
   definitions — syntactic sugar over existing local bindings ✅
3. **`def` for functions**: Replace global `fn name(params)` with `def name(params)`;
   change body syntax from `{ ... }` to `= expr;` — syntactic change to existing
   functionality ✅
4. **Curried parameter groups**: Allow multiple `(params)` groups on `lam`, `let`, `def`,
   and `fn` — purely syntactic, desugars to nested lambdas/pi types at parse time
5. **`def` for constants**: Allow parameter-free `def x: T = expr;` at the top level —
   requires new semantics (top-level constants do not currently exist)
6. **Default parameters**: Add `?=` once the design is finalised
