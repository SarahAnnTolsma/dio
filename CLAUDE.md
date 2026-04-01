# dio — JavaScript Deobfuscation Library

A high-performance JavaScript deobfuscation library written in Rust. Takes obfuscated JavaScript source code, applies a series of AST transformations to simplify and clean it, and returns pretty-printed output.

## Build & Test

```bash
cargo build              # Build all crates
cargo test               # Run all tests
cargo test -p dio-core   # Run core library tests only
cargo fmt                # Format all code
cargo clippy             # Lint all code
```

## Before Committing

All three checks must pass before committing. CI enforces these on every PR:

```bash
cargo fmt --all -- --check                # No formatting issues
cargo clippy --all -- -D warnings         # No clippy warnings (treated as errors)
cargo test --all                          # All tests pass
```

**Do not commit code with clippy warnings.** The CI pipeline uses `-D warnings` which treats all warnings as errors. Fix all warnings before committing — including pre-existing warnings in files you didn't change. If touching a file introduces or surfaces warnings, fix them all. No exceptions.

## Project Structure

- `crates/dio-core/` — Core deobfuscation library (all logic lives here)
- `crates/dio-cli/` — Command-line interface
- `crates/dio-ffi/` — C-compatible FFI for .NET, Java, C/C++
- `crates/dio-napi/` — Node.js native addon via napi-rs
- `crates/dio-wasm/` — WASM module for browsers

## Architecture

The main entry point is `Deobfuscator` in `dio-core`. It:
1. Parses JavaScript source using `oxc` (arena-allocated AST)
2. Builds scope/binding analysis via `oxc_semantic`
3. Runs a convergence loop dispatching AST nodes to registered `Transformer` implementations
4. After main transforms converge, runs a finalize phase (dead code elimination, renaming)
5. If finalize changes anything, restarts the main loop
6. Outputs pretty-printed code via `oxc_codegen`

## Code Style

- Full words, no abbreviations (e.g., `Expression` not `Expr`, `Identifier` not `Ident`)
- Types suffixed with their role (e.g., `MemberTransformer`, `ExpressionPattern`)
- Use `cargo fmt` for formatting
- Minimize `unsafe` code
- Doc comments on all public types and members
- Comments for non-obvious logic
- Member ordering: fields, constructors, getters/setters, methods; public before private within each group
