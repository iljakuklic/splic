# Self-typed core IR and `type_of`

## Context

During elaboration, `infer` currently returns a `(term, type)` pair where both
are `&'core core::Term<'core>`. The question was whether the elaborated IR could
be made *self-typed* — i.e. whether `type_of(term) -> Type` could be implemented
as a pure function on `Term` alone, removing the need to thread the type as a
second return value from `infer`.

## Which variants are already self-typed

Going through every `core::Term` variant:

| Variant | Type recoverable? | Notes |
|---|---|---|
| `Prim(IntTy(it))` | Yes | `U(it.phase)` |
| `Prim(U(Meta))` | Yes | `U(Meta)` (type-in-type for meta) |
| `Prim(U(Object))` | Yes | `U(Meta)` (object universe classified by meta) |
| `Prim(Add(it))` / arithmetic | Yes | `IntTy(it)` — same type in and out |
| `Prim(Eq(it))` / comparisons | Yes | `IntTy(U1, it.phase)` |
| `Lift(inner)` | Yes | Always `U(Meta)` |
| `Quote(inner)` | Yes (one recursive step) | `Lift(type_of(inner))` |
| `Splice(inner)` | Yes (one recursive step) | The `T` inside `Lift(T)` |
| `Let { ty, body, .. }` | Yes (one step into `body`) | `type_of(body)` |
| `Match { arms, .. }` | Yes (one step into first arm body) | `type_of(arms[0].body)`; all arms guaranteed same type post-elaboration |
| `App { head: Prim, .. }` | Yes | Same as `Prim` case above |
| `Lit(u64)` | **No** | Width was fixed by the `check` call; not stored in the node |
| `Var(Lvl)` | **No** | Needs the locals context (a slice indexed by level) |
| `App { head: Global(name), .. }` | **No** | Needs the globals table to look up `ret_ty` |

So three variants are not self-contained: `Lit`, `Var`, and `App/Global`.

## What the Kovács reference implementation does

The reference Haskell implementation (`https://github.com/AndrasKovacs/staged`)
does not attempt to make the syntactic `Tm` self-typed either. `infer` returns
`IO (Tm, VTy, Stage)` — a triple of the elaborated term, its **semantic value
type** (`VTy = Val`), and its stage. The type is threaded as a separate return
value throughout, not stored in the term.

The key design point is the `Tm` / `Val` split:

- **`Tm`** is the post-elaboration AST, using De Bruijn *indices*. Lambdas store
  a plain `Tm` body.
- **`Val`** is the result of evaluation. Lambdas become closures (`Val -> Val`),
  eliminating substitution. Variables become neutral spines (`VRigid Lvl Spine` /
  `VFlex MetaVar Spine`) using De Bruijn *levels*. `Let` is evaluated away
  immediately. Two terms are definitionally equal iff their `Val`s quote back to
  the same `Tm` (normalisation by evaluation).

Type-checking works with `Val` types throughout; `quote` converts back to `Tm`
when a syntactic form is needed (e.g. to store in the elaborated tree).

## Decision: keep the `(term, type)` pair for now

The current `infer :: … -> Result<(&'core Term, &'core Term)>` is the right shape
for now:

1. No structural change to `core::Term` is needed.
2. When dependent types and a normaliser arrive, the signature will naturally
   become `infer :: … -> Result<(&'core Term, Value)>` — matching the reference
   impl's `(Tm, VTy)` — and the transition is straightforward.

## Future direction: `type_of` on a self-typed IR

Eventually it may make sense for `infer` to return just `&'core Term`, with the
type recoverable on demand via `type_of(term, ctx) -> Value`. The elaborated term
would carry enough information that `type_of` is always cheap (O(1) field read or
a single eval step), with the context needed only for the `Var` lookup.

The only IR change required to reach that point is:

```
Lit(u64)  →  Lit(u64, IntType)
```

`IntType` is already in hand at the elaboration site (it is the `IntTy` inside
the `expected` type passed to `check`), so adding it costs nothing. All other
variants are already self-typed once `Var` has access to the locals context.

`Var` and `App/Global` still need external context for `type_of`, but that
context is a cheap slice/map lookup by level/name and does not require
re-elaboration.
