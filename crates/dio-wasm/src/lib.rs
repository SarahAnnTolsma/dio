//! WASM bindings for dio via wasm-bindgen.

use wasm_bindgen::prelude::*;

/// Deobfuscate JavaScript source code.
#[wasm_bindgen]
pub fn deobfuscate(source: &str) -> String {
    dio_core::deobfuscate(source)
}
