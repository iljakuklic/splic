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