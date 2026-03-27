# Kovács 2022 — *Staged Compilation with Two-Level Type Theory* (substantive rewrite)

This note rewrites the main implementation-relevant ideas.

## 1. Core 2LTT idea for staging

Two-level type theory provides:
- an **object theory** (runtime language) and
- a **meta theory** (compile-time language)

with a disciplined interface that ensures meta computation can be fully executed away, producing a splice-free object program.

### 1.1 Universes by stage
A common presentation is two universes:
- `U0` — object (runtime) types
- `U1` — meta (compile-time) types

There can also be size levels, but staging is orthogonal to sizing.

### 1.2 Stage-local type formers
Type formers typically exist separately at each stage. A guiding restriction:

> introduction/elimination forms for a stage stay in that stage

Examples:
- if `Nat0 : U0`, then recursion/induction on `Nat0` only produces terms in stage 0
- meta recursion/induction produces meta terms

This forces computation to be explicit and avoids "accidental" compile-time execution of runtime computation.

---

## 2. The three primitives: lift/quote/splice

The bridge between stages is not by ordinary functions, but by dedicated constructs:

### 2.1 Lift (type former)
For `A : U0`, there is a meta type:
- `⇑A : U1`

Interpretation: `⇑A` is "meta programs that produce object terms of type `A`".

### 2.2 Quote (term former)
For `t : A` where `A : U0`:
- `⟨t⟩ : ⇑A`

Quote is the trivial code producer: it returns `t` as code.

### 2.3 Splice (term former)
For `u : ⇑A`:
- `∼u : A`

Splice runs `u` during unstaging and inserts its produced object term into the surrounding object term.

### 2.4 Computation rules
Quote and splice cancel definitionally:
- `∼⟨t⟩` computes to `t`
- `⟨∼u⟩` computes to `u`

A practical compiler will implement these via evaluation/normalization of meta terms and/or definitional equality.

---

## 3. What "staging"/"unstaging" computes

Given a closed mixed-stage object term `t : A` at stage 0, unstaging:
- executes all meta computations inside splices
- replaces each splice by the computed object term
- produces a splice-free object term and type

A key point: this applies to **types too** (unrestricted staging for types), so meta code can compute object types.

---

## 4. Inlining control, partial evaluation, and why duplication happens

Because lifted code `⇑A` is a meta value, using it multiple times duplicates code.

This is useful for inlining, but can also duplicate runtime computations.

A typical pitfall (also emphasized later in the 2024 paper):
- converting between `⇑(A×B)` and `⇑A×⇑B` can duplicate uses of the underlying code unless you insert a let.

### 4.1 Let-insertion as an engineering requirement
A standard technique is to define meta-level combinators that generate object `let` bindings so the produced object code shares results.

The paper discusses ad-hoc let insertion and notes that more systematic let insertion can be built meta-theoretically (e.g. with continuation-based codegen).

---

## 5. Staging-by-evaluation (algorithmic blueprint)

The paper presents an algorithmic view:
- staging is "like normalization-by-evaluation"
- instead of reducing syntax directly, you interpret terms in a semantic domain where staging happens "by running the interpreter"

### 5.1 Soundness / stability / strictness (properties)
The paper identifies three desirable properties of a staging algorithm:

- **Soundness**: embedding the staged output recovers the original (up to conversion)
- **Stability**: staging an already object-level term is the identity (up to conversion)
- **Strictness**: staging preserves constructors strictly (not merely propositionally)

In implementation terms:
- you want `unstage(embed(obj)) == obj`
- and `embed(unstage(mixed))` converts back to `mixed`

### 5.2 Why the presheaf perspective matters (implementation takeaway)
To unstage open terms that depend on object variables, the semantic domain must support:
- object-variable dependency
- substitution/weakening stability ("naturality")

Implementation translation:
- treat object vars as neutrals
- meta evaluation may produce code that mentions those neutrals
- unstaging should commute with object substitution

---

## 6. Limits: intensional code analysis clashes with stability

The paper explains that if the meta language can *inspect* object code (e.g. decide definitional equality of two quoted boolean expressions), that generally breaks the substitution stability/naturality constraints that make the standard semantics work cleanly.

Implementation takeaway:
- if you want the clean semantic story and robust unstaging, keep object code opaque:
  only build it (quote), compose it (meta functions), and run it (splice),
  but do not pattern match on its structure in the meta language.

(If you *do* want inspection, you'll need a different setup and should expect trade-offs.)

---

## 7. Implementation Architecture: NbE + Staging

Modern practical implementations use **Normalization by Evaluation (NbE)** for type checking and evaluation for staging.

### Type Checker NbE (Kovács 2022 §3–4, elaboration-zoo 01-eval-closures-debruijn)

The type checker maintains a **semantic domain** separate from syntax:

```haskell
-- Haskell pseudocode (elaboration-zoo style)
data Value
  = VRigid Lvl Spine              -- stuck on a local variable
  | VLam Name (Val -> Val)         -- closure as a function
  | VPi  Name (Val -> Val) VTy    -- dependent Pi with closure
  | VLit Int
  | VGlobal Name
  | VLift Val
  | VQuote Val

-- Evaluation: interpret terms in an environment
eval :: Env Val -> Term -> Val
eval env (Var ix)     = env !! (env.len - 1 - ix)  -- index to stack
eval env (Lam x t)    = VLam x (\v -> eval (v:env) t)
eval env (Pi x a b)   = VPi x (\v -> eval (v:env) b) (eval env a)
eval env (App f args) = vApp (eval env f) (map (eval env) args)

-- Quotation: convert value back to term (for errors, output)
quote :: Lvl -> Val -> Term
quote lvl (VRigid x sp)  = quoteSp lvl (Var (lvl2Ix lvl x)) sp
quote lvl (VLam x t)     = Lam x (quote (lvl+1) (t (VRigid lvl)))
quote lvl (VPi x a b)    = Pi x (quote lvl a) (quote (lvl+1) (b (VRigid lvl)))
```

**Key design choices:**
- Terms use **De Bruijn indices** (count from nearest binder).
- Values use **De Bruijn levels** (count from outermost binder).
- Closures are functions `Val -> Val` in the metalanguage (or `Closure { env, body }` in Rust).
- No syntactic substitution — substitution is modeled via environment extension.

**Why this works:**
- Indices are pure syntax — portable, no external state.
- Levels are the natural output of evaluation — fresh variables are just the current depth.
- Closures capture the evaluation environment, eliminating variable-capture bugs.

### Staging Evaluator (Separate system)

The **staging evaluator** is a different system that compiles meta code and produces the object program:

```haskell
-- Two separate value types
data Val0 = V0Lit Int | V0App Name [Val0] | V0Code Term | ...
data Val1 = V1Lam (Val0 -> Val1) | V1Lit Int | ...

-- Two separate evaluators
eval0 :: Env Val0 -> Term -> Val0    -- object-level computation
eval1 :: Env Val1 -> Term -> Val1    -- meta-level computation
```

**Distinct from NbE because:**
- NbE normalizes types during type checking (unifies meta/object under `Value`).
- Staging separates meta and object code and produces the output program.
- The two systems operate on different goals with different value representations.

**In Splic:**
- Type checker: `core/value.rs` NbE (unified semantic domain, normalized for type comparison).
- Staging: `eval/mod.rs` with separate `Val0`/`Val1` (partitioned computation).

Both use the same `Term` representation and `Closure { env: &[Value], body: &Term }` pattern.

---

## 8. De Bruijn Representation and Shifting

### Indices vs Levels

Terms use **De Bruijn indices** (0 = nearest binder):

```
\x . \y . x   -->   Lam("x", Lam("y", Var(Ix(1))))
                    The reference to x is 1 step from the nearest binder (y).
```

Evaluation uses **De Bruijn levels** (0 = outermost):

```
context: [x : u64, y : u64, z : u64]   at depth 3
x is at level 0, y at level 1, z at level 2.
Fresh var is at level 3.

When quoting Rigid(1), convert to Var(Ix(3 - 1 - 1)) = Var(Ix(1)).
```

Conversions:
```rust
ix_to_lvl(depth: Lvl, ix: Ix) -> Lvl = Lvl(depth.0 - ix.0 - 1)
lvl_to_ix(depth: Lvl, lvl: Lvl) -> Ix = Ix(depth.0 - lvl.0 - 1)
```

### Free Variable Shifting (Staging)

When quoted code (`MetaVal::Code { term, depth }`) created at one depth is spliced at a deeper depth, its free variables must be shifted:

```
Code created at depth = 2: App(mul, [Var(Ix(0)), Var(Ix(1))])
Spliced at depth = 4: these indices now refer to different variables!

Solution: shift += (4 - 2), applied to free variables (Ix >= some cutoff).
```

Implement via recursive term traversal:

```rust
fn shift_free_ix(term, shift, cutoff) {
    match term {
        Var(Ix(i)) if i >= cutoff => Var(Ix(i + shift)),
        Lam { body, .. } => Lam { body: shift_free_ix(body, shift, cutoff) },
        // ... recursively apply to all sub-terms
    }
}
```

Only free variables (those not bound within the term itself) are shifted.

---

## 9. Reference Implementations

- **elaboration-zoo** (Kovács, 2020): https://github.com/AndrasKovacs/elaboration-zoo
  - Branch `01-eval-closures-debruijn` is the canonical reference for NbE + De Bruijn.
  - Haskell source is clean and readable; comments explain each step.
  - Shows the minimal NbE setup needed for dependent type checking.

- **2LTT skill / Splic** (this project):
  - `compiler/src/core/value.rs`: Core NbE data structures and functions.
  - `compiler/src/core/mod.rs`: De Bruijn index/level types and conversions.
  - `docs/bs/nbe_and_debruijn.md`: Detailed walkthrough of the architecture and index shifting.

- **Kovács papers**:
  - *Staged Compilation with Two-Level Type Theory* (ICFP 2022): Foundational theory and properties.
  - *Closure-Free Functional Programming in a Two-Level Type Theory* (ICFP 2024): Object-level closure optimization.

---

## 10. Glossary

| Term | Definition |
|------|-----------|
| **NbE** | Normalization by Evaluation. Interpreter-based type checking that maintains semantic values and quotes back to syntax. |
| **Closure** | `{ env: &[Value], body: &Term }`. Captured environment + unevaluated body for lazy evaluation. |
| **Neutral / Rigid** | A value that cannot be reduced further (e.g., stuck on a free variable). |
| **Canonical** | A value in "normal form" (fully evaluated). |
| **De Bruijn index** | Variable reference counting from the nearest binder (0 = innermost). |
| **De Bruijn level** | Variable position counting from the outermost binder (0 = root). |
| **Quote** | Convert a value back to term syntax. |
| **Free variable** | A variable not bound by any enclosing lambda/pi in the term. |
| **Shift** | Adjust De Bruijn indices when moving code to a different binding depth. |
| **Splice** | `$(e)`. Run meta code and insert the result into an object context. |
| **Quote** | `#(e)`. Embed an object term as meta code (lift to `[[T]]`). |
| **Lift** | `[[T]]`. Meta type of object code producing type `T`. |
