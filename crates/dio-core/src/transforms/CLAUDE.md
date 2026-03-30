# transforms/ — Built-in Transformer Implementations

Each transformer is grouped by category and implements the `Transformer` trait from `dio-core`. See the "How to Write a Transformer" section in `crates/dio-core/CLAUDE.md` for the full guide.

## Categories

- `constant/` — Constant folding and inlining (scope-aware).
- `string/` — String literal concatenation.
- `evaluation/` — Safe evaluation of known built-in functions (String.fromCharCode, parseInt, Number, Boolean, atob, btoa).
- `simplification/` — Block normalization, comma expressions, member access, control flow, ternary-to-if, sequence statement hoisting.
- `elimination/` — Dead code removal (finalize phase).
- `renaming/` — Scope-aware variable renaming (finalize phase).

## Conventions

- Each transformer struct is named `<Name>Transformer` (e.g., `ConstantFoldingTransformer`).
- Each transformer file is named `<name>_transformer.rs`.
- Transformers declare their `interests()` as specific `AstNodeType` variants.
- All AST mutations go through `operations::replace_expression`, `operations::replace_statement`, etc. — never assign directly.
- Always unwrap `ParenthesizedExpression` when matching operands or conditions.
- Use `operations::create_block_statement` when creating new block statements.
- Every transformer must have integration tests in the appropriate `crates/dio-core/tests/<category>.rs` file.
- Every transformer must be documented in `docs/Transformers.md` and the relevant category doc.
