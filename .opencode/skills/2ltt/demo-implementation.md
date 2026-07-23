# Reference implementation (Kovács `staged` demo)

Code-level reference for https://github.com/AndrasKovacs/staged/tree/main/demo — the
prototype accompanying the 2022 paper (theory: [`kovacs-2022-staged-compilation-2ltt.md`](kovacs-2022-staged-compilation-2ltt.md)).
Features: two stages, dependent functions, type-in-type (no sigma; data is
lambda-encoded), `Nat` at both stages, Agda-style implicits with higher-order
unification, and strong inference for staging operations.

Pipeline: parse → elaborate (bidirectional, NbE, metavariables) → zonk → stage.

## 1. Core syntax (Syntax.hs)

```hs
data Tm
  = Var Ix
  | Lam Name Icit Tm Tm Verbosity        -- λ (domain annotation, body)
  | App Tm Tm Icit Verbosity
  | Pi Name Icit Ty Ty
  | Let Stage Name Ty Tm Tm Verbosity    -- let with EXPLICIT stage (S0 | S1)
  | U Stage                              -- U0 / U1
  | Lift Ty                              -- ⇑A
  | Quote Tm                             -- ⟨t⟩   (surface syntax: <t>)
  | Splice Tm                            -- ∼t    (surface syntax: [t])
  | Nat Stage | Zero Stage | Suc Stage | NatElim Stage
  -- metavariable machinery (elaboration only; gone after zonking):
  | Meta MetaVar
  | InsertedMeta MetaVar Pruning         -- fresh meta applied to the bound-var mask
  | AppPruning Tm Pruning
  | Wk Tm                                -- explicit weakening, used by subtyping coercions
```

`Verbosity` (`V0`/`V1`) marks elaboration-inserted material for printing. Smart
constructors cancel quote/splice **already at elaboration time**:

```hs
tQuote (Splice t) = t ; tQuote t = Quote t
tSplice (Quote t) = t ; tSplice t = Splice t
```

## 2. Values for conversion checking (Value.hs, Evaluation.hs)

One shared semantic domain for elaboration-time evaluation (used by unification and
conversion). De Bruijn **indices** in terms, **levels** in values; closures are Haskell
functions `Val -> Val`.

```hs
data Spine = SId | SApp Spine ~Val Icit Verbosity
           | SSplice Spine                        -- stuck splice, as a spine entry
           | SNatElim Stage Val Val Val Spine

data Val
  = VFlex MetaVar Spine                  -- stuck on an unsolved meta
  | VRigid Lvl Spine                     -- stuck on a variable
  | VLam Name Icit ~VTy (Val -> Val) Verbosity
  | VPi Name Icit ~VTy (Val -> Val)
  | VU Stage | VLift Val | VQuote Val
  | VNat Stage | VZero Stage | VSuc Stage Val
```

Key evaluation cases — quote/splice cancellation on both canonical values and neutrals:

```hs
vSplice (VQuote t)    = t                              -- ∼⟨t⟩ = t
vSplice (VFlex m sp)  = VFlex m (SSplice sp)           -- stuck: push splice onto spine
vSplice (VRigid x sp) = VRigid x (SSplice sp)
vSplice _             = impossible                     -- (well-typed input)

vQuote (VFlex m (SSplice sp))  = VFlex m sp            -- ⟨∼n⟩ = n on neutrals
vQuote (VRigid x (SSplice sp)) = VRigid x sp
vQuote t                       = VQuote t              -- canonical
```

Takeaways for any NbE 2LTT checker:
- A stuck splice must be *representable* (here: `SSplice` spine entry on `VRigid`/`VFlex`).
  Without it, splices on neutrals have nowhere to go.
- Both cancellation directions are needed; the `⟨∼n⟩` direction fires when quoting a
  neutral whose spine ends in a splice.
- **This evaluator computes all redexes, object-level included** (the demo's object
  theory has full β) — conversion checking is stage-agnostic here. Contrast staging (§3)
  and the CFTT weak-equality alternative.

`quote :: Lvl -> Val -> Tm` reads values back (standard NbE); `nf = quote ∘ eval`.
`force` unfolds solved metas at the head. `zonk` inlines all solved metas into a term
without evaluating anything else — run before staging.

## 3. Staging (Staging.hs)

Separate pass over zonked core syntax, with **two value domains and two evaluators** —
meta values never appear in output, object values are the output:

```hs
data Env = Nil | Def0 Env Val0 | Def1 Env Val1   -- one mixed-stage environment

data Val1                       -- meta values: computed with, then discarded
  = VLam1 (Val1 -> Val1)
  | VQuote Val0                 -- ⟨t⟩ evaluates its body with eval0
  | VSomeU1                     -- ALL meta-level types erase to this dummy
  | VZero1 | VSuc1 Val1

data Val0                       -- object values: mirror the object syntax
  = VVar0 Lvl                   -- levels ⇒ no weakening/shifting ever
  | VApp0 Val0 Val0 Icit Verbosity
  | VLam0 Name Icit Val0 (Val0 -> Val0) Verbosity   -- closure per binder
  | VPi0  Name Icit Val0 (Val0 -> Val0)
  | VLet0 Name Val0 Val0 (Val0 -> Val0) Verbosity
  | VU0 | VNat0 | VZero0 | VSuc0 | VNatElim0
```

```hs
eval1 env (Lam _ _ _ t _) = VLam1 (eval1Bind env t)   -- β computed via vApp1
eval1 env (Quote t)       = VQuote (eval0 env t)      -- switch to object evaluation
eval1 env (U{}|Pi{}|Lift{}|Nat{}) = VSomeU1           -- types erased
eval1 env (Splice{})      = impossible                -- splice never in meta position

eval0 env (Lam x i a t o) = VLam0 x i (eval0 env a) (eval0Bind env t) o  -- NO β
eval0 env (App t u i o)   = VApp0 (eval0 env t) (eval0 env u) i o        -- structural
eval0 env (Splice t)      = vSplice (eval1 env t)     -- run metaprogram, embed result
  where vSplice (VQuote v) = v ; vSplice _ = impossible
eval0 env (Quote{})       = impossible

stage :: Tm -> Tm
stage t = quote0 0 (eval0 Nil t)      -- mixed-stage closed term → splice-free object term
```

Notes:
- `eval0` is "evaluation" only in the sense of resolving variables/splices — it copies
  object structure verbatim (strictness: no object β). It implements delayed renamings
  via closures + levels.
- Both evaluators error on `Meta`/`InsertedMeta`: **unsolved metas are a staging-time
  error** (with a hint to inspect `elab-verbose`).
- Variables in `Env` are stage-tagged (`Def0`/`Def1`); lookup projects the right domain.

## 4. Elaboration (Elaboration.hs, Cxt.hs)

Bidirectional: `check :: Cxt -> P.Tm -> VTy -> Stage -> IO Tm` and
`infer :: Cxt -> P.Tm -> IO (Tm, VTy, Stage)` — **infer returns the type *and* the
stage**. `inferS` is infer with an expected stage, reconciled via `adjustStage`.

### 4.1 Context (Cxt.hs)
`Cxt = { env :: [Val], lvl, path :: Path, pruning, srcNames :: Map Name (Lvl, VTy, Stage), pos }`.
Every binding records its **stage** in `srcNames`; `Path` is a context zipper used to
build closed Pi types for fresh metas cheaply. `bind` (bound var), `newBinder`
(elaboration-inserted, invisible to source), `define` (let).

### 4.2 Stage handling: repair, not reject
There is **no stage check at variable lookup** — `infer (Var x)` just returns the stored
stage. Mismatches between inferred and expected stage/type are *repaired* by coercive
subtyping (rules `A ≤ ⇑A`, `⇑A ≤ A`, `U0 ≤ U1`; theory in
[`kovacs-2022-staged-compilation-2ltt.md`](kovacs-2022-staged-compilation-2ltt.md) §3.3):

```hs
adjustStage cxt t a s s'    -- move (t : a : U s) to stage s'
  | s == s' = (t, a)
  | s <  s' = (tQuote t, VLift a)                    -- 0→1: quote
  | s >  s' = case force a of                        -- 1→0: splice
      VLift a -> (tSplice t, a)
      a       -> do m <- freshMeta (VU S0) S0        -- a must be ⇑?m
                    unifyCatch cxt a (VLift m)
                    (tSplice t, m)

coe cxt t a s a' s'         -- full coercion (t : a : U s) to (a' : U s')
  -- Pi vs Pi: contravariant/covariant, η-expanding with Wk for the shifted body;
  --   tracks "trivial coercion" (Nothing) to avoid inserting useless η-expansions
  -- (VU S0, VU S1)      -> Lift t                   -- U0 ≤ U1 witnessed by Lift
  -- (VLift a, VLift a') -> unify a a'
  -- (VLift a, a')       -> coe (tSplice t) ...      -- unwrap and retry
  -- (a, VLift a')       -> tQuote <$> coe t ...
  -- otherwise           -> adjustStage then unify
```

`Wk` (explicit weakening) exists solely so `coe` can reuse `t` under the binders it
introduces. Stage errors thus surface as *unification* failures, not as a dedicated
"wrong stage" error. (A simpler checker without subtyping can instead make stage
mismatch a hard error at lookup — a valid design choice, but not what this demo does.)

### 4.3 Notable check/infer cases

```hs
checkU cxt t s = check cxt t (VU s) s                -- "this must be a type at stage s"

check (P.Quote t) (VLift a) = tQuote <$> check t a S0
check t           (VLift a) = tQuote <$> check t a S0
  -- quote INSERTION: checking any non-quote against ⇑A recurses at stage 0.
  -- Loses no solutions: every value of ⇑A is ⟨t⟩ up to defeq. Major inference win.

check (P.Let st' x a t u) a' | st == st'             -- let stage must match the
  -- current stage; body elaborated in `define`d context

infer (P.Quote t)  = do (t, a) <- inferS t S0; pure (tQuote t, VLift a, S1)
infer (P.Splice t) = do (t, a) <- inferS t S1
                        (t, a) <- adjustStage t a S1 S0   -- forces a ≅ ⇑?m
                        pure (t, a, S0)
infer (P.App t u i) -- implicit insertion (insert'/insertUntilName), then:
  -- if head type isn't Pi, coerce it to a fresh Pi (coe) — subsumes stage repair
```

Fallback cases of `check` infer + `coe`. Implicit-argument insertion (`insert`,
`insert'`, `insertUntilName`) is standard elaboration-zoo style.

### 4.4 Design notes (from the demo README)
- **Stages must be unambiguous in source**: no stage metavariables/stage unification.
  Explicit stages on every `let` turn out to make the rest of inference highly effective;
  stage metavariables were tried in earlier prototypes and dropped as useless complexity.
- **Contextual metavariables** abstract over mixed-stage scopes — formally *outside*
  2LTT (no 2LTT type former crosses stages). Fine in practice; after zonking, pure 2LTT
  syntax remains. A fresh meta's type is a closed iterated Pi over its scope (`closeTy`).
- Coercion avoidance: `coe` returns `Nothing` for trivial coercions so e.g.
  `(Nat0 → Nat0) ≤ (Nat0 → Nat0)` doesn't η-expand.

## 5. Unification (Unification.hs)

Pattern unification with pruning, à la elaboration-zoo, extended for staging:
- spine inversion treats **quote/splice like a unary record's constructor/projection**
  (analogous to Σ projections), so metas can be solved under splices;
- meta η-expansion to eliminate splices from spines;
- intersection/pruning for nonlinear spines; occurs check; no postponed constraints;
- `unify` has `VLift/VLift` and `VQuote/VQuote` congruence cases; `solve` splits the
  spine at outer non-invertible entries (e.g. `SSplice`) and η-expands as needed.

## 6. What to copy vs. reconsider

Copy: smart constructors; stage-tagged bindings + `infer` returning stage; quote
insertion in `check`; stuck-splice spines; zonk-before-stage; unsolved-meta staging
error; two-domain staging with levels + closures; meta-type erasure in staging.

Reconsider per design: the coercive-subtyping repair (powerful but complex — a hard
stage error is the simple alternative); full object β in conversion (fits the 2022-style
object theory; a CFTT-style object language wants weak object equality instead — see
[`kovacs-2024-closure-free-2ltt.md`](kovacs-2024-closure-free-2ltt.md) §2.4); type-in-type (demo-only shortcut).
