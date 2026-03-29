# dio-wasm — WASM Module for Browsers

WebAssembly bindings for dio-core via wasm-bindgen. Provides `deobfuscate(source)` callable from browser JavaScript.

## Build

```bash
wasm-pack build --target web
```

## Usage (from browser)

```js
import init, { deobfuscate } from './dio-wasm';
await init();
const result = deobfuscate(obfuscatedCode);
```
