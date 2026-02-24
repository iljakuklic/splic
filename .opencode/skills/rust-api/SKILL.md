---
name: rust-api
description: Review Rust code for adherence to the official Rust API guidelines, including naming conventions, interoperability, documentation, predictability, flexibility, type safety, dependability, and future proofing.
compatibility: opencode
---

## What I do

I'm a reference for the official [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/) - a comprehensive set of recommendations for designing and presenting APIs for Rust. I help review code for adherence to these guidelines.

## Files to read (in order)

1. **[checklist.md](checklist.md)**
   - Concise checklist of all individual guidelines, suitable for quick scanning during code reviews
   - Covers: Naming, Interoperability, Macros, Documentation, Predictability, Flexibility, Type safety, Dependability, Debuggability, Future proofing, Necessities

2. **[about.md](about.md)**
   - Introduction and overview of the Rust API Guidelines
   - Explains the purpose and organization of the guidelines

3. **[naming.md](naming.md)**
   - Casing conventions (UpperCamelCase, snake_case, SCREAMING_SNAKE_CASE)
   - Getter naming, iterator method naming, feature naming, word order consistency

4. **[interoperability.md](interoperability.md)**
   - Implementing common traits (Copy, Clone, Eq, Hash, Debug, Display, etc.)
   - Conversion traits (From, AsRef, AsMut)
   - Collections implementing FromIterator and Extend
   - Serde integration, Send/Sync, error types, binary formatting

5. **[macros.md](macros.md)**
   - Input syntax that mirrors output
   - Macro composition with attributes
   - Item macros working anywhere, visibility specifiers, flexible type fragments

6. **[documentation.md](documentation.md)**
   - Crate-level docs with examples
   - Using `?` in examples (not unwrap)
   - Error, panic, and safety documentation sections
   - Hyperlinks, Cargo.toml metadata, release notes

7. **[predictability.md](predictability.md)**
   - Smart pointers not adding inherent methods
   - Conversions on the most specific type
   - Methods over functions with clear receivers
   - No out-parameters, operator overloads, Deref for smart pointers only

8. **[flexibility.md](flexibility.md)**
   - Exposing intermediate results
   - Caller controls data placement
   - Using generics to minimize assumptions
   - Object-safe traits

9. **[type-safety.md](type-safety.md)**
   - Newtypes for static distinctions
   - Custom types instead of bool/Option
   - bitflags for flag sets
   - Builder pattern for complex construction

10. **[dependability.md](dependability.md)**
    - Argument validation (static vs dynamic enforcement)
    - Destructors that never fail
    - Non-blocking destructor alternatives

11. **[debuggability.md](debuggability.md)**
    - All public types implementing Debug
    - Non-empty Debug representation

12. **[future-proofing.md](future-proofing.md)**
    - Sealed traits
    - Private struct fields
    - Newtypes hiding implementation details
    - Not duplicating derived trait bounds

13. **[necessities.md](necessities.md)**
    - Stable public dependencies
    - Permissive licensing (MIT/Apache-2.0)

14. **[external-links.md](external-links.md)**
    - Links to related RFCs and external resources

## When to use me

Use me when you are:
- Reviewing Rust crate code for API design quality
- Designing a new Rust library and want to follow best practices
- Checking if a crate follows established Rust conventions
- Looking up specific guidelines for naming, documentation, error handling, etc.
