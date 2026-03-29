# dio-napi — Node.js Native Addon

Node.js bindings for dio-core via napi-rs. Exposes `deobfuscate(source)` as a native function callable from JavaScript/TypeScript.

## Build

```bash
npm run build
```

## Usage (from Node.js)

```js
const { deobfuscate } = require('./dio-napi');
const result = deobfuscate(obfuscatedCode);
```
