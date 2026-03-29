# dio-core — Core Deobfuscation Library

All deobfuscation logic lives here. The other crates (cli, ffi, napi, wasm) are thin wrappers.

## Key Types

- `Deobfuscator` — Main entry point. Holds transformers, runs the convergence loop.
- `Transformer` (trait) — Implemented by each transform pass. Declares node interests, priority, and phase.
- `AstNodeType` — Enum of specific AST node types transformers can register interest in.
- `TransformerPriority` — `First` / `Default` / `Last` execution ordering.
- `TransformerPhase` — `Main` (convergence loop) or `Finalize` (post-convergence pruning).
- `TransformContext` — Wraps the oxc allocator and scoping info passed to transformers.
- `TransformDiagnostics` — Stats reported after deobfuscation (iterations, per-transformer counts).

## Modules

- `pattern/` — Declarative pattern matching on AST nodes with captures.
- `transforms/` — Built-in transformer implementations grouped by category.

## oxc Integration Notes

- oxc uses an arena allocator (`oxc_allocator::Allocator`). All AST nodes live in the arena.
- `oxc_traverse::traverse_mut` consumes and returns `Scoping` — use `std::mem::take` to move it in/out.
- New AST nodes are created via `AstBuilder` (from `TraverseCtx::ast()`), allocated in the arena.
- The `Traverse` trait has `enter_*`/`exit_*` methods for each AST node type.
