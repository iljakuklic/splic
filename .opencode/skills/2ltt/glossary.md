# Glossary

Shared terminology for the 2LTT skill. Referenced from
[`implementation-guide.md`](implementation-guide.md) and the paper notes.

| Term | Definition |
|------|-----------|
| **Stage** | 0 = object/runtime, 1 = meta/compile-time. Some implementations say "phase". |
| **Lift `⇑A`** | Meta type of metaprograms producing object code of type `A`. |
| **Quote `⟨t⟩`** | Staging intro: object term `t : A` as meta value of `⇑A`. |
| **Splice `∼t`** | Staging elim: run `t : ⇑A` during staging, insert resulting object term. |
| **Staging / unstaging** | Running all metaprograms; output is splice-free object code. Same operation, two names (2022 / 2024 papers). |
| **Soundness / stability / strictness** | Staging output ≈ input up to conversion / staging is identity on object code / staging preserves object term formers exactly. |
| **NbE** | Normalization by evaluation: eval syntax → semantic values, read back to syntax. |
| **Read-back ("quotation" in NbE jargon)** | `Lvl → Value → Term`. Not the staging quote. |
| **Neutral** | Value stuck on a variable/meta, carrying a spine of pending eliminations. |
| **Closure** | Captured environment + unevaluated body; applied by environment extension. |
| **De Bruijn index / level** | Count from nearest binder (syntax) / from outermost (values). |
| **Zonk** | Inline solved metavariables into a term. |
| **Binding-time improvement** | Rewriting toward meta-level structure (e.g. `⇑(A→B)` → `⇑A→⇑B`) so more computes at staging time. |
| **Generativity** | Metaprograms can't inspect object terms; internalizable as an axiom (CFTT). |
