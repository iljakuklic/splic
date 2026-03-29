# Practical implementation guide for 2LTT / CFTT-style systems

This file is the "do this in code" companion to the three paper rewrites in this skill.

## 0. Terminology (consistent with the papers)

- **Meta level** (compile time, stage 1): runs during unstaging.
- **Object level** (run time, stage 0): the output program after unstaging.

Core bridge primitives:

- `⇑A` (lift): a meta type meaning "code that will produce an object term of type `A`".
- `⟨t⟩` (quote): make code from an object term.
- `∼u` (splice): run meta code `u : ⇑A` and insert resulting object term into object term.

A crucial design choice:
- Meta level has "real computation" (β-reduction, recursion, induction, etc.).
- Object level is often treated as *code* during meta computation; definitional equality may be intentionally weak.

---

## 1. Minimal core syntax & judgments (implementation-oriented)

A practical way to structure the implementation is a *two-sorted* AST with a shared front-end.

### 1.1 Contexts

Keep two contexts (or a single context with a stage tag):
- `Δ` = meta context (compile-time variables)
- `Γ` = object context (runtime variables)

You will frequently need mixed judgments, e.g. meta terms that mention object variables *only through* quoted object syntax.

### 1.2 Object language (stage 0) AST (example)

You can start with a typed or untyped object AST. Typed helps invariants; untyped is simpler.

Example (untyped-ish, but with constructors you'll need):
- Variables / de Bruijn indices
- Let
- Lambdas + application (if your object language is higher-order)
- Data constructors + case
- (Optional) letrec for recursive functions

```hs
data ObjTm
  = OVar Ix
  | OLam Name ObjTy ObjTm
  | OApp ObjTm ObjTm
  | OLet Name ObjTy ObjTm ObjTm
  | OCon ConName [ObjTm]
  | OCase ObjTm [(Pat, ObjTm)]
  | OLetRec [(Name, ObjTy, ObjTm)] ObjTm
```

### 1.3 Meta language (stage 1) AST (example)

Meta level is a dependently typed λ-calculus (whatever subset you implement).

Add explicit constructors for:
- Lifted types: `Lift ObjTy`
- Quotation of object terms: `Quote ObjTm`
- Splice into object terms: represented **in the object AST** as `OSplice MetaTm`
  (or in a combined AST before you split).

A simple combined representation during elaboration:
```hs
data Tm
  = Var Ix
  | Lam Name Ty Tm
  | App Tm Tm
  | Pi Name Ty Ty
  | -- ...
  | Lift Ty0              -- ⇑A
  | Quote Tm0             -- ⟨t⟩
  | Splice Tm1            -- ∼u (only valid when typechecking an object term)
```

Then, after typechecking/elaboration, you can separate into `MetaTm` vs `ObjTmWithSplice` vs `ObjTm`.

---

## 2. Typing rules you actually need (core)

### 2.1 Stage separation invariants
Enforce:
- Ordinary type formers (Π/Σ/Id/inductives) do not mix stages "accidentally".
- Functions do not cross stages as ordinary terms.
- Interaction happens only via `⇑`, `⟨⟩`, `∼`.

### 2.2 Lift/Quote/Splice (schematic rules)

Let object types be `A : U0` (or `A : Ty`), meta types be `MetaTy` (or `U1`).

- Lift formation:
  - If `A` is an object type, then `⇑A` is a meta type.

- Quote introduction:
  - If `t` is an object term of type `A`, then `⟨t⟩` is a meta term of type `⇑A`.

- Splice elimination:
  - If `u` is a meta term of type `⇑A`, then `∼u` is an object term of type `A`.

- Definitional equalities (treat as computation rules):
  - `∼⟨t⟩  ≡  t`
  - `⟨∼u⟩  ≡  u`

In code, you'll likely implement these as *normalization* / *evaluation* rules in the meta evaluator and/or in conversion checking.

---

## 3. Definitional equality strategy (practical + matches the papers)

You need two conversion relations in practice:

### 3.1 Meta definitional equality (strong)
Meta definitional equality should compute:
- β for meta lambdas
- unfolding of meta let / (maybe) meta recursors
- computation for meta inductives / eliminators

This is how unstaging "runs".

### 3.2 Object definitional equality (weak)
To avoid needing an evaluator for the object language at compile time, you can intentionally set:

- **No β/η for object lambdas**
- No let-unfolding at the object level
- Possibly only α-equivalence / structural congruence

This is explicitly used in the 2024 closure-free paper to keep things simple when typechecking object code embedded in a dependent meta system.

---

## 4. Unstaging algorithm (the deliverable)

### 4.1 What unstaging does
Input: a well-typed "mixed-stage" program.
Output: a splice-free object program + splice-free object types.

### 4.2 Staging-by-evaluation (engineering form)

Implement a function (names vary):
- `unstageObj : MetaEnv -> ObjEnv -> ObjTmWithSplice -> ObjTm`
- `evalMeta : MetaEnv -> ObjEnv -> MetaTm -> MetaVal`

Key case: splicing inside object terms:
1. Evaluate the meta term `u : ⇑A` to a meta value that *represents code*.
2. Extract the produced object AST.
3. Recurse to ensure the result contains no splices.

Pseudocode:
```hs
unstageObj envM envO (OSplice u) =
  case evalMeta envM envO u of
    VCode a obj -> unstageObj envM envO obj
    _ -> error "impossible: splice must evaluate to code"

unstageObj envM envO (OLet x ty rhs body) =
  OLet x (unstageTy envM envO ty)
       (unstageObj envM envO rhs)
       (unstageObj envM (envO.ext x) body)

-- other constructors: recurse structurally
```

### 4.3 Representing code at the meta level
The easiest representation for values of type `⇑A` is literally:
```hs
data MetaVal
  = ...
  | VCode ObjTy ObjTmWithSplice  -- (optionally also carry A)
```
Then evaluation of `⟨t⟩` returns `VCode _ t`.

---

## 5. Let-insertion / sharing (needed for good codegen)

Naively, `down (up x)`-style conversions duplicate splices and can duplicate runtime work.

Solution pattern: a meta-level code-generation monad (CFTT uses `Gen`) that:
- introduces object-level `let` bindings to ensure sharing
- structures codegen in CPS so you can "emit lets" in order

You can implement a minimal writer-like CPS monad:
```hs
newtype Gen a = Gen { runGenK :: (a -> VCode) -> VCode }

gen   :: VCode -> Gen VCode    -- bind code to an object-level let, then pass variable code onward
runGen :: Gen VCode -> VCode
```

The key contract: `gen` makes sure the produced object code is a variable reference, so reusing it is cheap.

---

## 6. Closure-free object language (optional but central to Kovács 2024)

If you want a guarantee of "no dynamic closures", design the object language so that:
- **values** (data) cannot contain functions
- **computations** (functions) cannot be stored/passed around

Mechanically: split object types into:
- `ValTy` — types whose inhabitants are runtime values that can be stored
- `CompTy` — computations (not storable), e.g. functions, call-by-name-ish

Then enforce that constructor fields are in `ValTy`, and that function types live in `CompTy`.

This still permits lambdas under `case`/`let` syntactically, but compilation can use **call-saturation** / hoisting so calls are always saturated and closures are not needed.

---

## 7. Representation/calling convention indexing (optional; from KACC)

If you want "layout control" / "arity/eval-order control":
- add a kind/index level describing runtime representation and calling convention
- have object types carry these indices so codegen is type-directed

A minimal adaptation:
- `Rep` kind: pointer, int, float, unboxed tuple, etc.
- `Conv` kind: calling convention (arity, evaluation strategy)
- `TYPE rep conv` kind (or `Ty rep conv`) for object types whose runtime calling convention is known

Meta code can compute these indices and select specialized representations, and unstaging produces an IL-like typed program.

---

## 8. Implementation checklist

Core:
- [ ] Define two stages and enforce stage-local type formers.
- [ ] Implement `⇑`, `⟨⟩`, `∼` with correct typing.
- [ ] Implement strong meta normalization / evaluation.
- [ ] Implement weak object conversion (at least structural).
- [ ] Implement unstaging that eliminates all splices.

Quality:
- [ ] Add let-insertion (`Gen`) to prevent duplication.
- [ ] Decide if you want closure-free object typing (`ValTy`/`CompTy`).
- [ ] Decide if you want representation indices (KACC-style).

---

## 9. Implementation Architecture: NbE + Staging

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

## 10. De Bruijn Representation and Shifting

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

## 11. Reference Implementations

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

## 12. Bidirectional Elaboration Patterns (anti-drift guardrails)

The type-checker is a **bidirectional elaborator**: `infer` synthesises a type, `check` verifies one. Getting these right avoids a class of ad-hoc workarounds.

### 12.1 `infer` must return its type

```haskell
infer :: Ctx -> Stage -> Tm -> (CoreTm, VTy)
```

Returning the type directly means callers never need to reconstruct the type from the elaborated term. A helper like `typeOf` that pattern-matches the core term to recover a type is a signal that `infer` is not returning enough information.

### 12.2 `checkU` / `check_universe`

Instead of:
```haskell
(t, ty) <- infer ctx s e
unless (isUniverseType ty) $ fail "expected a type"
```

Use:
```haskell
t <- checkU ctx s e   -- checkU cxt t s = check cxt t (VU s) s
```

This directly encodes the kinding rule and avoids fragile `isUniverseType` predicates. See the reference implementation (`Elaboration.hs: checkU`).

### 12.3 Stuck splices are a neutral form

`Value` needs a `Splice` neutral alongside `Rigid` (stuck variable):

```
eval(Quote(Splice(v))) = v           -- cancel
eval(Splice(Quote(v))) = v           -- cancel
eval(Splice(v))        = Splice(v)   -- stuck: v is not a Quote
eval(Quote(v))         = Quote(v)    -- stuck: v is not a Splice
```

Without the `Splice` neutral, `eval(Splice(v))` has nowhere to go and either panics or silently drops the splice, breaking quote/splice cancellation in the NbE type-checker.

### 12.4 Stage-check variables at lookup

The context should record the stage of each binding and verify it matches the current elaboration stage when a variable is looked up:

```haskell
infer cxt (Var x) = do
  let (x', a, s) = lookupVar cxt x
  when (stage cxt /= s) $ fail "stage mismatch for variable"
  pure (Var x', a)
```

Without this, a meta-phase variable referenced in an object context produces a confusing type error instead of a clear stage error. The reference calls this `guardStage` (`Cxt.hs`).

---

## 13. Glossary

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
