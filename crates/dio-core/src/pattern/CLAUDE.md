# pattern/ — Declarative AST Pattern Matching

Provides a composable, enum-based pattern matching system for matching against oxc AST nodes.

## Key Types

- `ExpressionPattern` — Matches expression nodes. Variants for each expression type plus meta-patterns.
- `StatementPattern` — Matches statement nodes. Variants for each statement type plus meta-patterns.
- `MatchResult` — Result of a pattern match, containing whether it matched and any named captures.
- `CapturedNode` — A value extracted from a matched node (string, number, boolean, or node reference).

## Meta-Patterns

- `Any` — Matches any node of the appropriate type.
- `AnyLiteral` — Matches any literal expression.
- `Capture(name, inner)` — Matches inner pattern and stores the matched value under the given name.
- `Repeat(inner)` — Matches zero or more consecutive items (for use in statement lists).
- `And`, `Or`, `Not` — Logical combinators.
- `Predicate` — Escape hatch for custom matching logic.

## Combinators

Free functions in `combinators.rs` provide ergonomic construction: `identifier()`, `any()`, `capture()`, etc.
