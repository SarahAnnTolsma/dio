# transforms/ — Built-in Transformer Implementations

Each transformer is grouped by category and implements the `Transformer` trait from `dio-core`.

## Categories

- `constant/` — Constant folding and inlining (scope-aware).
- `string/` — String literal concatenation.
- `evaluation/` — Safe evaluation of known built-in functions (String.fromCharCode, parseInt, atob).
- `simplification/` — Comma expressions, member access, control flow simplification.
- `elimination/` — Dead code removal (finalize phase).
- `renaming/` — Scope-aware variable renaming (finalize phase).

## Conventions

- Each transformer struct is named `<Name>Transformer` (e.g., `ConstantFoldingTransformer`).
- Each transformer file is named `<name>_transformer.rs`.
- Transformers declare their `interests()` as specific `AstNodeType` variants.
- Internal visitor structs (implementing oxc `Traverse`) are private to each transformer.
- Transformers that modify scoping (inlining, renaming) run in appropriate phases/priorities.
