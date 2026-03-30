//! Integration tests for the BuiltinEvaluationTransformer.

mod common;
use common::deobfuscate;

// These tests verify that built-in function calls are evaluated to their
// constant results. The constant inliner then inlines the result into
// usage sites and the unused variable pruner removes the declaration.

#[test]
fn string_from_char_code() {
    assert_eq!(
        deobfuscate("var x = String.fromCharCode(72, 105); f(x);"),
        "f(\"Hi\");"
    );
}

#[test]
fn parse_int() {
    assert_eq!(deobfuscate("var x = parseInt(\"10\"); f(x);"), "f(10);");
}

#[test]
fn parse_int_hex() {
    assert_eq!(deobfuscate("var x = parseInt(\"ff\", 16); f(x);"), "f(255);");
}

#[test]
fn parse_float() {
    assert_eq!(deobfuscate("var x = parseFloat(\"3.14\"); f(x);"), "f(3.14);");
}

#[test]
fn number_parse_int() {
    assert_eq!(
        deobfuscate("var x = Number.parseInt(\"10\"); f(x);"),
        "f(10);"
    );
}

#[test]
fn number_parse_int_hex() {
    assert_eq!(
        deobfuscate("var x = Number.parseInt(\"ff\", 16); f(x);"),
        "f(255);"
    );
}

#[test]
fn number_parse_float() {
    assert_eq!(
        deobfuscate("var x = Number.parseFloat(\"3.14\"); f(x);"),
        "f(3.14);"
    );
}

#[test]
fn number_from_string() {
    assert_eq!(deobfuscate("var x = Number(\"42\"); f(x);"), "f(42);");
}

#[test]
fn number_from_float_string() {
    assert_eq!(deobfuscate("var x = Number(\"3.14\"); f(x);"), "f(3.14);");
}

#[test]
fn number_from_empty_string() {
    assert_eq!(deobfuscate("var x = Number(\"\"); f(x);"), "f(0);");
}

#[test]
fn number_from_true() {
    assert_eq!(deobfuscate("var x = Number(true); f(x);"), "f(1);");
}

#[test]
fn number_from_false() {
    assert_eq!(deobfuscate("var x = Number(false); f(x);"), "f(0);");
}

#[test]
fn number_from_null() {
    assert_eq!(deobfuscate("var x = Number(null); f(x);"), "f(0);");
}

#[test]
fn boolean_from_number_truthy() {
    assert_eq!(deobfuscate("var x = Boolean(1); f(x);"), "f(true);");
}

#[test]
fn boolean_from_number_falsy() {
    assert_eq!(deobfuscate("var x = Boolean(0); f(x);"), "f(false);");
}

#[test]
fn boolean_from_string_truthy() {
    assert_eq!(deobfuscate("var x = Boolean(\"hello\"); f(x);"), "f(true);");
}

#[test]
fn boolean_from_string_falsy() {
    assert_eq!(deobfuscate("var x = Boolean(\"\"); f(x);"), "f(false);");
}

#[test]
fn boolean_from_null() {
    assert_eq!(deobfuscate("var x = Boolean(null); f(x);"), "f(false);");
}

#[test]
fn atob() {
    assert_eq!(
        deobfuscate("var x = atob(\"SGVsbG8=\"); f(x);"),
        "f(\"Hello\");"
    );
}

#[test]
fn chained_string_from_char_code() {
    assert_eq!(
        deobfuscate(
            "var x = String.fromCharCode(116) + String.fromCharCode(101) + String.fromCharCode(115) + String.fromCharCode(116); f(x);"
        ),
        "f(\"test\");"
    );
}
