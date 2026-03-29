# dio-cli — Command-Line Interface

Thin CLI wrapper around `dio-core`. Reads JavaScript from a file or stdin, deobfuscates it, and writes to stdout or a file.

## Usage

```bash
dio input.js                  # deobfuscate, print to stdout
dio input.js -o output.js     # deobfuscate, write to file
dio --max-iterations 50 input.js
dio --diagnostics input.js    # print transform stats to stderr
cat input.js | dio -          # read from stdin
```
