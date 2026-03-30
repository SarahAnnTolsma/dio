# Renaming Transformers

## VariableRenamingTransformer

**Status: Stub** - not yet implemented.

Will rename obfuscated variable names to short, readable names using scope analysis. Targets identifiers that look obfuscated (hex-style names, underscore-prefixed hex, etc.).

Runs in the **Finalize** phase with **Last** priority so that all other transforms complete first.

```js
// Before
var _0x4a3f = 10;
var _0x1b2c = _0x4a3f + 5;
console.log(_0x1b2c);

// After (planned)
var a = 10;
var b = a + 5;
console.log(b);
```

### Planned behavior

- Walk the scope tree from oxc semantic analysis
- Identify bindings with obfuscated-looking names (hex patterns, long random strings)
- Generate short names (`a`, `b`, `c`, ..., `aa`, `ab`, ...) scoped per block
- Rename all references to each binding using `operations::rename_binding`
- Avoid collisions with existing names in the same scope
