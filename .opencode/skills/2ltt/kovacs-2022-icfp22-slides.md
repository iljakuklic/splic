# Kovács (ICFP 2022 slides) — selected implementation notes (rules + Vec + inlining + inference)

This document includes **only** the implementation-relevant material from the following slides:
- Rules of 2LTT
- map with inlining
- Inference for staging operations
- Staging types (Vec / Tuple3)
- map for Vec

It’s intended as a compact companion to the fuller ICFP’22 paper notes in this skill.

---

## 1) Rules of 2LTT (core calculus you must implement)

1. **Two universes** `U0`, `U1`, closed under arbitrary type formers.
   - `U0` is the universe of runtime (object-level) types.
   - `U1` is the universe of compile-time (meta-level) types.

2. **Stage separation:** all type/term formers and eliminators stay within the same universe.

3. **Lifting:** for `A : U0`, we have `⇑A : U1`.

4. **Quoting:** for `A : U0` and `t : A`, we have `<t> : ⇑A`.

5. **Splicing:** for `t : ⇑A`, we have `∼t : A`.

6. **Computation laws:**
   - `<∼t> ≡ t`
   - `∼<t> ≡ t`

**Operational staging principle:** staging runs all metaprograms in splices and inserts their result in the code output.

---

## 2) Example: `map` with inlining (staged higher-order programming)

### Input
```text
inlMap :
  {A B : ⇑U0} →
  (⇑∼A → ⇑∼B) →
  ⇑(List0 ∼A) → ⇑(List0 ∼B)

inlMap =
  λ f as.
    <foldr0 (λ a bs. cons0 ∼(f <a>) bs) nil0 ∼as>

f : List0 Nat0 → List0 Nat0
f = λ xs. ∼(inlMap (λ n. <∼n + 2>) <xs>)
```

### Output (after staging)
```text
f : List0 Nat0 → List0 Nat0
f = λ xs. foldr0 (λ a bs. cons0 (a + 2) bs) nil0 xs
```

**Implementation check:** this exercises that you can (1) pass a meta-level function producing code, and (2) eliminate all splices by evaluating meta code, yielding pure object code.

---

## 3) Inference for staging operations (reduce annotation burden)

### 3.1 Definitional isomorphisms (negative types)
Lifting preserves negative types up to definitional isomorphism:

```text
⇑ ⊤0 ≃ ⊤1
⇑ ((a : A) → B a) ≃ ((a : ⇑A) → ⇑(B ∼a))
⇑ ((a : A) × B a) ≃ ((a : ⇑A) × ⇑(B ∼a))
```

### 3.2 Elaboration strategy suggested by the slides
Use:
- **bidirectional elaboration**, and
- **coercive subtyping along these isomorphisms**

to infer most quotes/splices automatically.

The slide’s “post-inference” presentation:

```text
inlMap : {A B : ⇑U0} → (⇑A → ⇑B) → ⇑(List0 A) → ⇑(List0 B)
inlMap = λ f. foldr0 (λ a bs. cons0 (f a) bs) nil0

f : List0 Nat0 → List0 Nat0
f = inlMap (λ n. n + 2)
```

**Implementation note:** a practical approach is:
- keep a core language with explicit `< >` / `∼`,
- elaborate from a surface language where many are implicit,
- insert coercions (or treat these isos as definitional equalities in conversion).

---

## 4) Staging types: `Vec` computed at compile time, spliced into `U0`

### Input
```text
Vec : Nat1 → ⇑U0 → ⇑U0
Vec zero1  A = <⊤0>
Vec (suc1 n) A = <∼A × ∼(Vec n A)>

Tuple3 : U0 → U0
Tuple3 A = ∼(Vec 3 <A>)
```

### Output (after staging)
```text
Tuple3 : U0 → U0
Tuple3 A = A × (A × (A × ⊤0))
```

**Implementation check:** staging must normalize meta computation that produces *object types* (i.e. produce a splice-free `U0` type).

---

## 5) `map` for staged `Vec` (dependent-ish staging across `Nat1`)

### Input
```text
map : {A B : ⇑U0} → (n : Nat1) → (⇑∼A → ⇑∼B)
    → ⇑(Vec n A) → ⇑(Vec n B)

map zero1     f as = <tt0>
map (suc1 n)  f as = <(∼(f <fst0 ∼as>), ∼(map n f <snd0 ∼as>))>

f : ∼(Vec 2 <Nat0>) → ∼(Vec 2 <Nat0>)
f xs = ∼(map 2 (λ x. <∼x + 2>) <xs>)
```

### Output (after staging)
```text
f : Nat0 × (Nat0 × ⊤0) → Nat0 × (Nat0 × ⊤0)
f xs = (fst0 xs + 2, (fst0 (snd0 xs) + 2, tt0))
```

**Implementation check:** this confirms that:
- meta recursion over `Nat1` expands the `Vec` shape,
- object projections (`fst0`, `snd0`) appear in the final object code,
- splices are fully eliminated.
