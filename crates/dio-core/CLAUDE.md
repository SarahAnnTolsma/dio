# dio-core — Core Deobfuscation Library

All deobfuscation logic lives here. The other crates (cli, ffi, napi, wasm) are thin wrappers.

## Key Types

- `Deobfuscator` — Main entry point. Holds transformers, runs the convergence loop.
- `Transformer` (trait) — Implemented by each transform pass. Declares node interests, priority, and phase.
- `AstNodeType` — Enum of specific AST node types transformers can register interest in.
- `TransformerPriority` — `First` / `Default` / `Last` execution ordering.
- `TransformerPhase` — `Main` (convergence loop) or `Finalize` (post-convergence pruning).
- `operations` — Scope-aware AST mutation functions (`replace_expression`, `replace_statement`, `remove_statement`, `rename_binding`, etc.). Transformers must use these instead of direct assignment.
- `TransformDiagnostics` — Stats reported after deobfuscation (iterations, per-transformer counts).

## Modules

- `pattern/` — Declarative pattern matching on AST nodes with captures.
- `transforms/` — Built-in transformer implementations grouped by category.

## How to Write a Transformer

### 1. Create the file

Add a new file in the appropriate `transforms/<category>/` directory, named `<name>_transformer.rs`. Create a struct named `<Name>Transformer` and implement the `Transformer` trait.

### 2. Implement the trait

```rust
use crate::operations;
use crate::transformer::{AstNodeType, Transformer, TransformerPhase, TransformerPriority};

pub struct MyTransformer;

impl Transformer for MyTransformer {
    fn name(&self) -> &str { "MyTransformer" }
    fn interests(&self) -> &[AstNodeType] { &[AstNodeType::CallExpression] }
    fn priority(&self) -> TransformerPriority { TransformerPriority::Default }
    fn phase(&self) -> TransformerPhase { TransformerPhase::Main }

    fn enter_expression<'a>(
        &self,
        expression: &mut Expression<'a>,
        context: &mut TraverseCtx<'a, ()>,
    ) -> bool {
        // Return true if you modified the AST, false otherwise.
        false
    }
}
```

Choose the right hook:
- `enter_expression` / `exit_expression` — for expression-level transforms.
- `enter_statement` / `exit_statement` — for statement-level transforms.
- `enter_statements` — for transforms that need to insert, remove, or splice statements in a list (requires `AstNodeType::StatementList` interest).

### 3. Use the operations module for all AST mutations

Never directly assign to `*expression` or `*statement`. Always use:

- `operations::replace_expression(target, replacement, context)` — replace an expression.
- `operations::replace_statement(target, replacement, context)` — replace a statement.
- `operations::remove_expression(target, context)` — replace with `void 0`.
- `operations::remove_statement(target, context)` — replace with empty statement.
- `operations::remove_statement_at(statements, index, context)` — remove from a list.
- `operations::retain_statements(statements, predicate, context)` — filter a list.
- `operations::rename_binding(symbol_id, new_name, context)` — rename a binding.
- `operations::create_block_statement(body, context)` — create a block with a proper scope ID.
- `operations::insert_statement` / `append_statement` — add to a list.
- `operations::insert_expression` / `append_expression` — add to an expression list.

These functions keep oxc's scoping data in sync. Direct assignment orphans identifier references and corrupts scope analysis.

### 4. Watch out for ParenthesizedExpression

oxc's parser preserves parentheses as `ParenthesizedExpression` nodes by default. When matching operands or conditions, always unwrap parens first:

```rust
// WRONG — will miss `(1 + 2) * 3` because left is ParenthesizedExpression, not NumericLiteral
if let (Expression::NumericLiteral(l), Expression::NumericLiteral(r)) = (&binary.left, &binary.right) { ... }

// RIGHT — look through parens
fn unwrap_parens<'a, 'b>(expr: &'b Expression<'a>) -> &'b Expression<'a> {
    let mut current = expr;
    while let Expression::ParenthesizedExpression(paren) = current {
        current = &paren.expression;
    }
    current
}
if let (Expression::NumericLiteral(l), Expression::NumericLiteral(r)) =
    (unwrap_parens(&binary.left), unwrap_parens(&binary.right)) { ... }
```

This applies to conditions in if/ternary, operands in binary expressions, arguments in function calls, and any other context where the parser may have wrapped an expression in parens.

### 5. Creating new BlockStatements

When creating `BlockStatement` nodes (e.g., for if/else bodies), always use `operations::create_block_statement`. Never use `context.ast.statement_block()` directly — it creates blocks without a scope ID, which causes oxc's traversal to panic.

```rust
// WRONG — panics during traversal
let block = context.ast.statement_block(SPAN, body);

// RIGHT — registers a child scope
let block = operations::create_block_statement(body, context);
```

### 6. Register the transformer

1. Add `mod my_transformer;` and `pub use my_transformer::MyTransformer;` to the category's `mod.rs`.
2. Add `Box::new(category::MyTransformer)` to `default_transformers()` in `transforms/mod.rs`.
3. If the transformer needs a new `AstNodeType` variant, add it to `transformer.rs` and the classifier in `deobfuscator.rs`.

### 7. Add tests

Every new transformer must have integration tests. Tests are organized by transformer category in `crates/dio-core/tests/`:

- `constant_folding.rs` — ConstantFoldingTransformer (including JSFuck coercion)
- `constant_inlining.rs` — ConstantInliningTransformer
- `string_concatenation.rs` — StringConcatenationTransformer
- `builtin_evaluation.rs` — BuiltinEvaluationTransformer (including Math methods)
- `literal_method_evaluation.rs` — LiteralMethodEvaluationTransformer
- `control_flow.rs` — ControlFlowTransformer (including empty block simplification)
- `bitwise_simplification.rs` — BitwiseSimplificationTransformer
- `global_alias_simplification.rs` — GlobalAliasSimplificationTransformer
- `simplification.rs` — BlockNormalization, Comma, TernaryToIf, LogicalToIf, SequenceStatement, Member (including assignment LHS), VariableDeclarationSplit
- `function_declaration.rs` — FunctionDeclarationTransformer
- `proxy_function_inlining.rs` — ProxyFunctionInliningTransformer
- `dead_code.rs` — DeadCodeTransformer (including side-effect-free statement removal)
- `string_array_decoder.rs` — StringArrayDecoderTransformer (Obfuscator.io preset)
- `control_flow_array.rs` — ControlFlowArrayTransformer (Obfuscator.io preset)
- `set_timeout_unwrap.rs` — SetTimeoutUnwrapTransformer (DataDome preset)
- `combined.rs` — Multi-transformer interaction tests
- `presets.rs` — Preset selection and configuration tests

All test files use `mod common; use common::deobfuscate;` for the shared helper. Tests should:

- Cover the primary transformation (happy path).
- Cover edge cases (no-op when the pattern doesn't match).
- Cover interaction with other transformers (e.g., constant folding feeds into control flow) in `combined.rs`.
- Use the `deobfuscate()` helper which trims output for comparison.

### 8. Update documentation

When adding a new transformer, update the documentation in `docs/`:

- Add the transformer to the table in `docs/Transformers.md`.
- Add input/output examples to the appropriate category document (e.g., `docs/Simplification.md`).
- If creating a new category, create a new `docs/<Category>.md` and link it from `Transformers.md`.

## oxc Integration Notes

- oxc uses an arena allocator (`oxc_allocator::Allocator`). All AST nodes live in the arena.
- `oxc_traverse::traverse_mut` consumes and returns `Scoping` — the deobfuscator builds it once at startup and passes it through all traversals.
- New AST nodes are created via `AstBuilder` (from `TraverseCtx::ast`), allocated in the arena.
- The `Traverse` trait has `enter_*`/`exit_*` methods for each AST node type.
- Scoping is kept in sync by the `operations` module — there is no between-pass semantic rebuild.
- Transformers must never directly assign to `*expression` or `*statement`; use `operations::replace_expression`, `operations::replace_statement`, etc.
