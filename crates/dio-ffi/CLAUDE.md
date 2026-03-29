# dio-ffi — C-Compatible FFI Bindings

Exposes dio-core functionality via `extern "C"` functions for use from C, C++, .NET (P/Invoke), and Java (JNI/JNA).

## Exported Functions

- `dio_deobfuscate(source: *const c_char) -> *mut c_char` — Deobfuscate JS source. Caller must free with `dio_free_string`.
- `dio_free_string(s: *mut c_char)` — Free a string returned by dio.
