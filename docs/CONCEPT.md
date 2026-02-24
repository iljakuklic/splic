# Concept: Splic

This document describes the high-level vision for **Splic**, a novel programming language targeting zkVMs.

## Why a new language?

Writing low-level zkvm bytecode directly is tedious and error-prone. Existing high-level languages either abstract away too much (losing fine-grained control over the generated proof) or do not provide enough abstraction to express complex invariants.

This language sits at the sweet spot: high-level abstractions for productivity, with direct control over the generated zkvm bytecode.

## Two-level types

The language is built on **two-level type theory (2LTT)**: a dependently typed meta-language for writing programs, and a separate low-level object-language for the zkvm bytecode.

The **meta-level** is a purely functional dependently typed language. Programs express invariants in types and build complex zkvm code through a typed quoting mechanism. Dependent types enable encoding rich specifications that the typechecker verifies.

The **object-level** is a low-level language exposing primitive operations and zkvm-specific constructs. It has explicit control flow: the programmer sees exactly what bytecode will be generated.

The two levels are connected through **quotations** (producing object-level code from meta-level expressions) and **splices** (embedding object-level code into meta-level programs). This provides type-safe metaprogramming: manipulation and composition of zkvm code with full static guarantees.

## Syntax

The high-level syntax takes inspiration from Rust: familiar, readable, with good ergonomics.

Main differences from Rust:
- Added **Quotations and splices** for two-level types
- **No syntactic separation** between type-level and term-level expressions to support dependent types

## Goals

This is an experimental language with an unlimited weirdness budget. We're not trying to be conservative or compatible with anything. The goal is to explore what becomes possible when programmers receive both high-level abstraction power and low-level control over zkvm code generation.
