# Splic

**Splic** is an experimental programming language for writing zero-knowledge virtual machine (zkVM) code. It bridges the gap between high-level abstractions and low-level control, providing a strongly-typed language with two-level type theory (2LTT) that enables type-safe metaprogramming and fine-grained code generation.

The language is built on research in staged compilation and dependent types, enabling programmers to write efficient, proven-correct bytecode while maintaining high-level expressiveness. With an unlimited weirdness budget, Splic explores novel approaches to zkVM programming without backward compatibility constraints.

## Documentation

- **[INTRO.md](docs/INTRO.md)** – User-facing introduction to Splic with motivation, features, and examples
- **[SYNTAX.md](docs/SYNTAX.md)** – Complete language syntax reference and design principles
- **[PROTOTYPE.md](docs/prototype.md)** – Specification of the current prototype implementation

## Building and Testing

```bash
cargo build -p splic-compiler
cargo test -p splic-compiler
```

## CLI

```bash
cargo run -p splic-cli -- stage <FILE>
```

Stages a Splic source file, printing the object-level code with all meta-level computation resolved.

See `AGENTS.md` for detailed development commands and project structure.
