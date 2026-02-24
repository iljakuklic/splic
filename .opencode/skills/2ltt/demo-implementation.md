# Demo implementation (Kovács staged)

This file contains code snippets from the reference implementation at:
https://github.com/AndrasKovacs/staged/tree/main/demo

## 1. Core types (Common.hs)

```hs
data Stage = S0 | S1   -- S0 = object (runtime), S1 = meta (compile-time)

newtype Ix  = Ix {unIx :: Int}   -- De Bruijn index
newtype Lvl = Lvl {unLvl :: Int} -- De Bruijn level
newtype MetaVar = MetaVar {unMetaVar :: Int}
```

## 2. Syntax (Syntax.hs)

```hs
data Tm
  = Var Ix
  | Lam Name Icit Tm Tm Verbosity          -- lambda
  | App Tm Tm Icit Verbosity               -- application
  | Pi Name Icit Ty Ty                     -- dependent function
  | Let Stage Name Ty Tm Tm Verbosity      -- let (with stage)
  
  | U Stage                                -- universes
  | Quote Tm                               -- ⟨t⟩ 
  | Splice Tm                              -- [t] (splice)
  | Lift Ty                                -- ⇑A
  
  | Nat Stage                              -- natural numbers
  | Zero Stage
  | Suc Stage
  | NatElim Stage                          -- dependent elimination
  deriving Show
```

Key differences from paper notation:
- Paper uses `~t` for splice, demo uses `[t]`
- `Let` is annotated with a `Stage` (S0 or S1)

## 3. Semantic values for meta evaluation (Value.hs)

```hs
data Spine
  = SId
  | SApp Spine Val Icit Verbosity
  | SSplice Spine
  | SNatElim Stage Val Val Val Spine

data Val
  = VFlex MetaVar Spine                    -- unsolved meta variable
  | VRigid Lvl Spine                       -- variable
  | VLam Name Icit VTy (Val -> Val) Verbosity
  | VPi Name Icit VTy (Val -> Val)
  | VU Stage
  | VLift Val                              -- lifted value
  | VQuote Val                             -- quoted object term
  | VNat Stage
  | VZero Stage
  | VSuc Stage Val
```

The `Spine` tracks pending applications and special operations during evaluation.

## 4. Meta evaluator (Evaluation.hs)

```hs
vApp :: Val -> Val -> Icit -> Verbosity -> Val
vApp t ~u i o = case t of
  VLam _ _ _ t o -> t u
  VFlex  m sp    -> VFlex m  (SApp sp u i o)
  VRigid x sp    -> VRigid x (SApp sp u i o)
  _              -> impossible

vQuote :: Val -> Val
vQuote = \case
  VFlex  m (SSplice sp) -> VFlex m sp          -- quote/splice cancel
  VRigid x (SSplice sp) -> VRigid x sp
  t                     -> VQuote t

vSplice :: Val -> Val
vSplice = \case
  VQuote t    -> t                              -- splice of quote
  VFlex m sp  -> VFlex m (SSplice sp)
  VRigid x sp -> VRigid x (SSplice sp)
  _           -> impossible

eval :: Env -> Tm -> Val
eval env = \case
  Var x             -> vVar env x
  App t u i vr      -> vApp (eval env t) (eval env u) i vr
  Lam x i a t vr    -> VLam x i (eval env a) (evalBind env t) vr
  Pi x i a b        -> VPi x i (eval env a) (evalBind env b)
  Let _ _ _ t u _   -> eval (env :> eval env t) u
  U s               -> VU s
  Quote t           -> vQuote (eval env t)
  Splice t          -> vSplice (eval env t)
  Lift t            -> VLift (eval env t)
  -- ... nat eliminator handling
```

Quotation (value back to syntax):
```hs
quote :: Lvl -> Val -> Tm
quote l t = case force t of
  VFlex m sp     -> quoteSp l (Meta m) sp
  VRigid x sp    -> quoteSp l (Var (lvl2Ix l x)) sp
  VLam x i a t o -> Lam x i (quote l a) (quote (l + 1) (t (VVar l))) o
  VPi x i a b    -> Pi x i (quote l a) (quote (l + 1) (b (VVar l)))
  VU s           -> U s
  VLift t        -> Lift (quote l t)
  VQuote t       -> Quote (quote l t)
  VNat s         -> Nat s
  -- ...
```

## 5. Staging / Unstaging (Staging.hs)

The staging module separates meta and object evaluation:

```hs
data Env = Nil | Def0 Env Val0 | Def1 Env Val1

data Val1  -- meta (compile-time) values
  = VLam1 (Val1 -> Val1)
  | VQuote Val0
  | VSomeU1                 -- meta types ignored during staging
  | VZero1
  | VSuc1 Val1

data Val0  -- object (runtime) values
  = VVar0 Lvl
  | VApp0 Val0 Val0 Icit Verbosity
  | VPi0 Name Icit Val0 (Val0 -> Val0)
  | VLam0 Name Icit Val0 (Val0 -> Val0) Verbosity
  | VLet0 Name Val0 Val0 (Val0 -> Val0) Verbosity
  | VU0
  | VNat0
  | VZero0
  | VSuc0
  | VNatElim0
```

Meta evaluation (only runs at compile time):
```hs
eval1 :: Env -> Tm -> Val1
eval1 env = \case
  Var x             -> vVar1 env x
  Lam x i a t o    -> VLam1 (eval1Bind env t)
  App t u i o      -> vApp1 (eval1 env t) (eval1 env u)
  Quote t          -> VQuote (eval0 env t)    -- quote object to meta
  -- ...
  Splice{}          -> impossible              -- splices only in object context
  Lift{}            -> VSomeU1
```

Object evaluation (staging output):
```hs
eval0 :: Env -> Tm -> Val0
eval0 env = \case
  Var x            -> vVar0 env x
  Lam x i a t o    -> VLam0 x i (eval0 env a) (eval0Bind env t) o
  App t u i o      -> VApp0 (eval0 env t) (eval0 env u) i o
  Splice t         -> vSplice (eval1 env t)   -- splice evaluates meta, embeds object
  -- ...
  Quote{}          -> impossible              -- quotes only in meta context
```

Unstaging entry point:
```hs
stage :: Tm -> Tm
stage t = quote0 0 $ eval0 Nil t
```

The key staging invariant: `stage` takes a mixed-stage term and produces a splice-free object term.

## 6. Key design notes

- **Two separate value types**: `Val1` for meta-level computation, `Val0` for object-level code. They never mix.
- **Quote/Splice as primitives**: Quote converts object → meta (as value), splice runs meta and embeds object.
- **Stage-annotated let**: `Let S0` for object lets, `Let S1` for meta lets.
- **No object beta reduction during staging**: object lambdas are opaque; only meta computation runs.
- **Bidirectional elaboration**: separate checking and inference passes.
- **Implicit inference**: uses Agda-style implicit arguments with higher-order unification.
