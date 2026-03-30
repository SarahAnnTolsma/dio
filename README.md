# dio

A high-performance JavaScript deobfuscation library written in Rust. Takes obfuscated JavaScript source code, applies a series of AST transformations to simplify and clean it, and returns pretty-printed output.

## Features

- **Constant folding** — `1 + 2` → `3`, `!true` → `false`, `typeof "x"` → `"string"`
- **Constant inlining** — `const x = 5; f(x)` → `f(5)` (scope-aware)
- **String concatenation** — `"a" + "b" + "c"` → `"abc"`
- **Built-in evaluation** — `String.fromCharCode(72)` → `"H"`, `parseInt("1a", 16)` → `26`, `atob("aGVsbG8=")` → `"hello"`
- **Comma simplification** — `(1, 2, x)` → `x`
- **Member simplification** — `obj["prop"]` → `obj.prop`
- **Control flow simplification** — `if (true) A else B` → `A`, `true ? a : b` → `a`
- **Dead code elimination** — Removes unreachable code after `return`, `throw`, `break`, `continue`
- **Variable renaming** — Renames obfuscated identifiers (`_0x4a3f` → `a`, `b`, `c`) using scope analysis
- **User-extensible** — Add your own transformers via the `Transformer` trait

## Installation

### Rust

Add to your `Cargo.toml`:

```toml
[dependencies]
dio-core = { git = "https://github.com/SarahAnnTolsma/dio" }
```

### CLI

```bash
cargo install --git https://github.com/SarahAnnTolsma/dio dio-cli
```

### Node.js

```bash
npm install @dio/node
```

### WASM (Browser)

```bash
npm install @dio/wasm
```

## Usage

### CLI

```bash
# Deobfuscate a file, print to stdout
dio input.js

# Write to a file
dio input.js -o output.js

# Read from stdin
cat input.js | dio -

# Limit iterations and show diagnostics
dio --max-iterations 50 --diagnostics input.js
```

### Rust API

```rust
use dio_core::deobfuscate;

let source = r#"var a = 1 + 2; var b = "hello" + " " + "world";"#;
let result = deobfuscate(source);
println!("{}", result);
// var a = 3;
// var b = "hello world";
```

For more control, use the `Deobfuscator` builder:

```rust
use dio_core::Deobfuscator;

let result = Deobfuscator::new()
    .with_max_iterations(50)
    .with_diagnostics_callback(|diagnostics| {
        eprintln!("{}", diagnostics);
    })
    .deobfuscate(source);
```

### Custom Transformers

Implement the `Transformer` trait to add your own transform passes:

```rust
use dio_core::{Deobfuscator, Transformer, AstNodeType, TransformerPriority, TransformerPhase};

struct MyTransformer;

impl Transformer for MyTransformer {
    fn name(&self) -> &str { "MyTransformer" }
    fn interests(&self) -> &[AstNodeType] { &[AstNodeType::CallExpression] }
    fn priority(&self) -> TransformerPriority { TransformerPriority::Default }
    fn phase(&self) -> TransformerPhase { TransformerPhase::Main }

    // Implement enter_expression, enter_statement, etc.
}

let result = Deobfuscator::empty()
    .add_transformer(Box::new(MyTransformer))
    .deobfuscate(source);
```

## Architecture

dio uses [oxc](https://github.com/oxc-project/oxc) for parsing, semantic analysis, AST traversal, and code generation. The deobfuscation pipeline:

1. Parse JavaScript source into an arena-allocated AST
2. Build scope and binding analysis
3. Run a **convergence loop** dispatching AST nodes to registered transformers
4. After transforms converge, run a **finalize phase** (dead code elimination, renaming)
5. If finalize changes anything, restart the main loop
6. Output pretty-printed code

Transformers declare interest in specific AST node types and are dispatched only when relevant nodes are visited, keeping traversal efficient.

## Platform Support

| Platform | Crate | Mechanism |
|----------|-------|-----------|
| Rust | `dio-core` | Native library |
| C/C++ | `dio-ffi` | `extern "C"` functions |
| .NET | `dio-ffi` | P/Invoke |
| Java | `dio-ffi` | JNI/JNA |
| Node.js | `dio-napi` | napi-rs native addon |
| Browser | `dio-wasm` | wasm-bindgen |

## Building

```bash
cargo build                # Build all crates
cargo test                 # Run all tests
cargo test -p dio-core     # Core library tests only
```

## License

[MIT](LICENSE)
