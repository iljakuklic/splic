# Comparison Operators

With dependent types, comparisons have two natural interpretations:

1. **Boolean function**: `a < b` returns `u1` (boolean), a computed value
2. **Proposition**: `a < b` returns `Type`, a type-level assertion (proof)

## Current Syntax

The current grammar uses `==`, `!=`, `<`, `>`, `<=`, `>=` for comparison operators, returning `u1`.

## Open Questions

### Option 1: Separate Syntax

Use different operators for boolean vs proposition:
- Boolean: `<?`, `==?`, `<=?`, etc.
- Proposition: `<`, `==`, `<=`, etc.

### Option 2: Overloading

Same syntax, context determines return type:
- In term position: returns `u1`
- In type position: returns `Type`

Requires type inference to disambiguate.

### Option 3: Unification

Some way to unify both interpretations—likely complex.

## Decision

Deferred until dependent types are added. Current syntax is provisional.
