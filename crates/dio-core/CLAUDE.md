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

## oxc Integration Notes

- oxc uses an arena allocator (`oxc_allocator::Allocator`). All AST nodes live in the arena.
- `oxc_traverse::traverse_mut` consumes and returns `Scoping` — the deobfuscator builds it once at startup and passes it through all traversals.
- New AST nodes are created via `AstBuilder` (from `TraverseCtx::ast`), allocated in the arena.
- The `Traverse` trait has `enter_*`/`exit_*` methods for each AST node type.
- Scoping is kept in sync by the `operations` module — there is no between-pass semantic rebuild.
- Transformers must never directly assign to `*expression` or `*statement`; use `operations::replace_expression`, `operations::replace_statement`, etc.
