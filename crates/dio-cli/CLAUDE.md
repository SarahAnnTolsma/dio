# dio-cli — Command-Line Interface

Thin CLI wrapper around `dio-core`. Reads JavaScript from a file or stdin, deobfuscates it, and writes to stdout or a file.

## Usage

```bash
dio input.js                              # deobfuscate, print to stdout
dio input.js -o output.js                 # deobfuscate, write to file
dio --preset obfuscator-io input.js       # use a specific preset
dio --preset jsfuck encoded.js            # JSFuck-specific transforms
dio --max-iterations 50 input.js
dio --diagnostics input.js                # print transform stats to stderr
cat input.js | dio -                      # read from stdin
```

## Presets

Available presets via `--preset`:
- `generic` (default) — full transformer set for general deobfuscation
- `obfuscator-io` — targets Obfuscator.io / javascript-obfuscator
- `jsfuck` — focused subset for JSFuck-encoded JavaScript
