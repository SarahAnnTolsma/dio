# Renaming Transformers

## VariableRenamingTransformer

Renames obfuscated variable names to short, readable names using scope analysis. Targets identifiers that look obfuscated (hex-style names, underscore-prefixed hex, etc.).

Runs in the **Finalize** phase with **Last** priority so that all other transforms complete first.

```js
// Before
var _0x4a3f = 10;
var _0x1b2c = _0x4a3f + 5;
console.log(_0x1b2c);

// After
var a = 10;
var b = a + 5;
console.log(b);
```

### Behavior

- Walks the scope tree from oxc semantic analysis
- Identifies bindings with obfuscated-looking names (hex patterns, long random strings)
- Generates short names (`a`, `b`, `c`, ..., `aa`, `ab`, ...) scoped per block
- Renames all references to each binding using `operations::rename_binding`
- Avoids collisions with existing names in the same scope
