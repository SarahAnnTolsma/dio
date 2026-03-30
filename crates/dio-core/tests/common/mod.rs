use dio_core::Deobfuscator;

/// Deobfuscate and trim trailing whitespace/newlines for comparison.
pub fn deobfuscate(source: &str) -> String {
    Deobfuscator::new().deobfuscate(source).trim().to_string()
}
