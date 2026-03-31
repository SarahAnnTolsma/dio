//! Tests for transformer presets.

use dio_core::{Deobfuscator, Preset};

fn deobfuscate_with(preset: Preset, source: &str) -> String {
    Deobfuscator::with_preset(preset)
        .deobfuscate(source)
        .trim()
        .to_string()
}

// ---------------------------------------------------------------------------
// Preset::Generic
// ---------------------------------------------------------------------------

#[test]
fn generic_preset_matches_default() {
    let source = "var x = 1 + 2;";
    let default_result = Deobfuscator::new().deobfuscate(source);
    let preset_result = Deobfuscator::with_preset(Preset::Generic).deobfuscate(source);
    assert_eq!(default_result, preset_result);
}

// ---------------------------------------------------------------------------
// Preset::ObfuscatorIo
// ---------------------------------------------------------------------------

#[test]
fn obfuscator_io_folds_constants() {
    assert_eq!(deobfuscate_with(Preset::ObfuscatorIo, "var x = 1 + 2; f(x);"), "f(3);");
}

#[test]
fn obfuscator_io_inlines_proxy_functions() {
    assert_eq!(
        deobfuscate_with(
            Preset::ObfuscatorIo,
            "function _0x1(a, b) { return a + b; } var x = _0x1(1, 2); f(x);"
        ),
        "f(3);"
    );
}

// ---------------------------------------------------------------------------
// Preset::JsFuck
// ---------------------------------------------------------------------------

#[test]
fn jsfuck_folds_coercion() {
    assert_eq!(deobfuscate_with(Preset::JsFuck, "var x = +[];"), "var x = 0;");
}

#[test]
fn jsfuck_double_not_array() {
    assert_eq!(deobfuscate_with(Preset::JsFuck, "var x = !![];"), "var x = true;");
}

#[test]
fn jsfuck_plus_true() {
    assert_eq!(deobfuscate_with(Preset::JsFuck, "var x = +!![];"), "var x = 1;");
}

// ---------------------------------------------------------------------------
// Preset::from_name
// ---------------------------------------------------------------------------

#[test]
fn preset_from_name_valid() {
    assert_eq!(Preset::from_name("generic"), Some(Preset::Generic));
    assert_eq!(Preset::from_name("default"), Some(Preset::Generic));
    assert_eq!(Preset::from_name("obfuscator-io"), Some(Preset::ObfuscatorIo));
    assert_eq!(Preset::from_name("obfuscator_io"), Some(Preset::ObfuscatorIo));
    assert_eq!(
        Preset::from_name("javascript-obfuscator"),
        Some(Preset::ObfuscatorIo)
    );
    assert_eq!(Preset::from_name("datadome"), Some(Preset::DataDome));
    assert_eq!(Preset::from_name("data-dome"), Some(Preset::DataDome));
    assert_eq!(Preset::from_name("jsfuck"), Some(Preset::JsFuck));
}

#[test]
fn preset_from_name_case_insensitive() {
    assert_eq!(Preset::from_name("JsFuck"), Some(Preset::JsFuck));
    assert_eq!(Preset::from_name("OBFUSCATOR-IO"), Some(Preset::ObfuscatorIo));
    assert_eq!(Preset::from_name("Generic"), Some(Preset::Generic));
}

#[test]
fn preset_from_name_unknown() {
    assert_eq!(Preset::from_name("unknown"), None);
    assert_eq!(Preset::from_name(""), None);
}

// ---------------------------------------------------------------------------
// add_transformers API
// ---------------------------------------------------------------------------

#[test]
fn add_transformers_to_empty() {
    let mut deobfuscator = Deobfuscator::empty();
    deobfuscator.add_transformers(dio_core::obfuscator_io_transformers());
    let result = deobfuscator.deobfuscate("var x = 1 + 2; f(x);").trim().to_string();
    assert_eq!(result, "f(3);");
}
