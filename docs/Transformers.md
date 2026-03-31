# Transformers

dio applies a series of AST transformers to simplify and deobfuscate JavaScript. Transformers run in two phases:

- **Main** phase runs in a convergence loop until no transformer makes changes.
- **Finalize** phase runs once after the main loop converges. If it makes changes, the main loop restarts.

## General-Purpose Transformer Groups

These transformers are included in all presets (except JsFuck, which uses a focused subset).

### [Constant](Constant.md)

Transformers that evaluate or propagate constant values.

| Transformer | Status | Phase | Description |
|---|---|---|---|
| ConstantFoldingTransformer | Active | Main | Folds constant arithmetic, comparisons, typeof, void, and type coercion (JSFuck patterns) |
| ConstantInliningTransformer | Active | Main | Inlines single-assignment constants (var/let/const with no write references) into their references |

### [Evaluation](Evaluation.md)

Transformers that evaluate known built-in function calls.

| Transformer | Status | Phase | Description |
|---|---|---|---|
| BuiltinEvaluationTransformer | Active | Main | Evaluates pure built-in functions with constant arguments (parseInt, Number, Boolean, atob, btoa, Math methods) |
| LiteralMethodEvaluationTransformer | Active | Main | Evaluates method calls and property access on string/array literals |

### [String](String.md)

Transformers that simplify string operations.

| Transformer | Status | Phase | Description |
|---|---|---|---|
| StringConcatenationTransformer | Active | Main | Concatenates adjacent string literal additions |

### [Simplification](Simplification.md)

Transformers that normalize and simplify control flow and expressions.

| Transformer | Status | Phase | Description |
|---|---|---|---|
| GlobalAliasSimplificationTransformer | Active | Main | Replaces member access through `window`/`self`/`globalThis` aliases with direct globals |
| BitwiseSimplificationTransformer | Active | Main | Simplifies MBA expressions via truth table evaluation |
| BlockNormalizationTransformer | Active | Main | Wraps bare control flow bodies in block statements |
| CommaTransformer | Active | Main | Removes side-effect-free leading expressions from sequences |
| ControlFlowTransformer | Active | Main | Simplifies if/else and ternaries with constant conditions; removes empty if/else branches |
| FunctionDeclarationTransformer | Active | Main | Converts `var x = function() {}` to `function x() {}` |
| MemberTransformer | Active | Main | Converts computed member access to dot notation (expression and assignment LHS) |
| LogicalToIfTransformer | Active | Main | Converts standalone logical &&/\|\| expressions to if statements |
| SequenceStatementTransformer | Active | Main | Splits sequence expressions in expression statements and hoists leading expressions from sequences in return/if/while/throw/switch/for |
| TernaryToIfTransformer | Active | Main | Converts standalone ternary expressions to if/else |
| VariableDeclarationSplitTransformer | Active | Main | Splits multi-declarator variable declarations into individual statements |

### [Inlining](Inlining.md)

Transformers that inline function calls and remove indirection.

| Transformer | Status | Phase | Description |
|---|---|---|---|
| ProxyFunctionInliningTransformer | Active | Main | Inlines proxy functions that wrap a binary operation, call forwarding, or identity |

### [Elimination](Elimination.md)

Transformers that remove dead or unreachable code.

| Transformer | Status | Phase | Description |
|---|---|---|---|
| DeadCodeTransformer | Active | Finalize | Removes unreachable code after return/throw/break/continue and side-effect-free expression statements |
| UnusedVariableTransformer | Active | Finalize | Removes unused variable declarations |

### [Renaming](Renaming.md)

Transformers that rename obfuscated identifiers.

| Transformer | Status | Phase | Description |
|---|---|---|---|
| VariableRenamingTransformer | Active | Finalize | Renames obfuscated variable names to short readable names |

## Preset-Specific Transformer Groups

These transformers are only enabled when using a specific preset.

### Obfuscator.io (`--preset obfuscator-io`)

| Transformer | Description |
|---|---|
| StringArrayDecoderTransformer | Decodes string arrays with atob or custom base64 alphabets |
| StringArrayRotationTransformer | Solves array rotation and inlines plain-text string lookups |
| StringArrayRC4DecoderTransformer | Decodes RC4-encrypted string arrays (high obfuscation mode) |
| ControlFlowArrayTransformer | Resolves 2D control flow dispatch arrays built with hash functions |

### DataDome (`--preset datadome`)

Extends Obfuscator.io with DataDome-specific patterns.

| Transformer | Description |
|---|---|
| SetTimeoutUnwrapTransformer | Unwraps `setTimeout(function() { x = value; }, 0)` into direct assignments |

## Presets

Presets provide curated transformer sets optimized for specific obfuscation tools.

| Preset | CLI flag | Description |
|---|---|---|
| Generic | `--preset generic` | Default transformer set — handles common patterns across many tools |
| ObfuscatorIo | `--preset obfuscator-io` | Targets Obfuscator.io / javascript-obfuscator output |
| DataDome | `--preset datadome` | Extends Obfuscator.io with DataDome anti-bot script patterns |
| JsFuck | `--preset jsfuck` | Focused subset for JSFuck-encoded JavaScript |

### Usage

```rust
// Rust API
use dio_core::{Deobfuscator, Preset};

let deobfuscator = Deobfuscator::with_preset(Preset::ObfuscatorIo);

// Or build from scratch:
let mut deobfuscator = Deobfuscator::empty();
deobfuscator.add_transformers(dio_core::obfuscator_io_transformers());
```

```bash
# CLI
dio --preset obfuscator-io input.js -o output.js
dio --preset datadome input.js -o output.js
dio --preset jsfuck encoded.js
```
