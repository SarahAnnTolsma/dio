//! Node.js bindings for dio via napi-rs.

use napi_derive::napi;

/// Deobfuscate JavaScript source code using default settings.
#[napi]
pub fn deobfuscate(source: String) -> String {
    dio_core::deobfuscate(&source)
}

/// Deobfuscate JavaScript source code with custom options.
#[napi]
pub fn deobfuscate_with_options(source: String, max_iterations: Option<u32>) -> String {
    let mut deobfuscator = dio_core::Deobfuscator::new();
    if let Some(max) = max_iterations {
        deobfuscator = deobfuscator.with_max_iterations(max as usize);
    }
    deobfuscator.deobfuscate(&source)
}
