# dio-cli — Command-Line Interface

Thin CLI wrapper around `dio-core`. Reads JavaScript from a file or stdin, deobfuscates it, and writes to stdout or a file.

## Usage

```bash
dio input.js                                       # deobfuscate, print to stdout
dio input.js -o output.js                           # deobfuscate, write to file
dio --presets obfuscator-io input.js                 # use a specific preset
dio --presets datadome input.js                      # DataDome anti-bot scripts
dio --presets datadome,debundler input.js             # combine multiple presets
dio --presets jsfuck encoded.js                      # JSFuck-specific transforms
dio --max-iterations 50 input.js                     # limit convergence loop iterations
dio --diagnostics input.js                           # print transform stats to stderr
cat input.js | dio -                                 # read from stdin
```

## Presets

Available presets via `--presets` (comma-separated):
- `generic` (default) — full transformer set for general deobfuscation
- `obfuscator-io` — targets Obfuscator.io / javascript-obfuscator
- `datadome` — extends obfuscator-io with DataDome anti-bot patterns
- `debundler` — annotates Browserify modules with named functions and JSDoc types
- `jsfuck` — focused subset for JSFuck-encoded JavaScript

Multiple presets can be combined: `--presets datadome,debundler`
