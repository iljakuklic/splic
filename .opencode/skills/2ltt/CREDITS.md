# Credits / sources

This skill is derived from (and intended to be read alongside) the following sources.

## Primary sources

1. András Kovács.
   *Staged Compilation with Two-Level Type Theory.*
   Proc. ACM Program. Lang. 6, ICFP, Article 110 (August 2022), 30 pages.
   DOI: 10.1145/3547641
   PDF: https://andraskovacs.github.io/pdfs/2ltt.pdf
   (LaTeX source: `icfp22paper/` in the repository below.)

2. András Kovács.
   *Closure-Free Functional Programming in a Two-Level Type Theory.*
   Proc. ACM Program. Lang. 8, ICFP, Article 259 (August 2024), 34 pages.
   DOI: 10.1145/3674648
   PDF: https://andraskovacs.github.io/pdfs/2ltt_icfp24.pdf
   (LaTeX source and Agda/Haskell supplements: `icfp24paper/` in the repository below.)

3. Paul Downen, Zena M. Ariola, Simon Peyton Jones, and Richard A. Eisenberg.
   *Kinds Are Calling Conventions.*
   Proc. ACM Program. Lang. 4, ICFP, Article 104 (August 2020), 51 pages.
   DOI: 10.1145/3408986
   PDF: https://pauldownen.com/publications/kacc.pdf

4. András Kovács.
   *ICFP 2022 presentation slides: Staged Compilation with Two-Level Type Theory.*
   Presented 12 September 2022, ICFP Ljubljana.
   https://github.com/AndrasKovacs/staged/blob/main/icfp22prez/ICFP-Kov%C3%A1cs-StagedCompilationwithTwoLevelTypeTheory.pdf
   (Content folded into `kovacs-2022-staged-compilation-2ltt.md`; no separate notes file.)

## Reference implementations

5. András Kovács. *staged* — demo implementation, paper sources, Agda embeddings.
   https://github.com/AndrasKovacs/staged
   (The `demo/` directory and its `README.md` are the basis of `demo-implementation.md`.)

6. András Kovács. *elaboration-zoo* — minimal NbE / bidirectional elaboration
   references (see branch `01-eval-closures-debruijn` and later branches for
   metavariables and implicits).
   https://github.com/AndrasKovacs/elaboration-zoo

## Background

7. Danil Annenkov, Paolo Capriotti, Nicolai Kraus, Christian Sattler.
   *Two-Level Type Theory and Applications.* (origin of 2LTT; homotopy-type-theory
   motivation, cofibrancy)
   https://arxiv.org/abs/1705.03307
