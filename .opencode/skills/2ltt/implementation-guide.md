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
