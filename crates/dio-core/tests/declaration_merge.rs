//! Integration tests for the DeclarationMergeTransformer.

mod common;
use common::deobfuscate;

#[test]
fn merge_numeric_assignment() {
    assert_eq!(deobfuscate("var x; x = 42; f(x);"), "f(42);");
}

#[test]
fn merge_negative_numeric() {
    assert_eq!(deobfuscate("var x; x = -418; f(x);"), "f(-418);");
}

#[test]
fn merge_string_assignment() {
    assert_eq!(deobfuscate("var s; s = \"hello\"; f(s);"), "f(\"hello\");");
}

#[test]
fn merge_boolean_assignment() {
    assert_eq!(deobfuscate("var b; b = true; f(b);"), "f(true);");
}

#[test]
fn merge_null_assignment() {
    assert_eq!(deobfuscate("var n; n = null; f(n);"), "f(null);");
}

#[test]
fn merge_multiple_declarations() {
    assert_eq!(
        deobfuscate("var a; var b; a = 1; b = 2; f(a, b);"),
        "f(1, 2);"
    );
}

#[test]
fn skip_non_literal_rhs() {
    // Non-literal right-hand side should not be merged.
    assert_eq!(
        deobfuscate("var x; x = foo(); f(x);"),
        "var x;\nx = foo();\nf(x);"
    );
}

#[test]
fn skip_multiple_writes() {
    // Variables with more than one write should not be merged.
    assert_eq!(
        deobfuscate("var x; x = 1; x = 2; f(x);"),
        "var x;\nx = 1;\nx = 2;\nf(x);"
    );
}

#[test]
fn skip_already_initialized() {
    // Already initialized declarations should not be merged.
    assert_eq!(deobfuscate("var x = 1; f(x);"), "f(1);");
}

#[test]
fn non_adjacent_merge() {
    // Assignment doesn't have to be immediately after the declaration.
    assert_eq!(deobfuscate("var x; f(); x = 5; g(x);"), "f();\ng(5);");
}
