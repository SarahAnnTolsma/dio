//! Integration tests for the BuiltinEvaluationTransformer.

mod common;
use common::deobfuscate;

#[test]
fn string_from_char_code() {
    assert_eq!(
        deobfuscate("var x = String.fromCharCode(72, 105);"),
        "var x = \"Hi\";"
    );
}

#[test]
fn parse_int() {
    assert_eq!(deobfuscate("var x = parseInt(\"10\");"), "var x = 10;");
}

#[test]
fn parse_int_hex() {
    assert_eq!(deobfuscate("var x = parseInt(\"ff\", 16);"), "var x = 255;");
}

#[test]
fn parse_float() {
    assert_eq!(deobfuscate("var x = parseFloat(\"3.14\");"), "var x = 3.14;");
}

#[test]
fn number_parse_int() {
    assert_eq!(
        deobfuscate("var x = Number.parseInt(\"10\");"),
        "var x = 10;"
    );
}

#[test]
fn number_parse_int_hex() {
    assert_eq!(
        deobfuscate("var x = Number.parseInt(\"ff\", 16);"),
        "var x = 255;"
    );
}

#[test]
fn number_parse_float() {
    assert_eq!(
        deobfuscate("var x = Number.parseFloat(\"3.14\");"),
        "var x = 3.14;"
    );
}

#[test]
fn number_from_string() {
    assert_eq!(deobfuscate("var x = Number(\"42\");"), "var x = 42;");
}

#[test]
fn number_from_float_string() {
    assert_eq!(deobfuscate("var x = Number(\"3.14\");"), "var x = 3.14;");
}

#[test]
fn number_from_empty_string() {
    assert_eq!(deobfuscate("var x = Number(\"\");"), "var x = 0;");
}

#[test]
fn number_from_true() {
    assert_eq!(deobfuscate("var x = Number(true);"), "var x = 1;");
}

#[test]
fn number_from_false() {
    assert_eq!(deobfuscate("var x = Number(false);"), "var x = 0;");
}

#[test]
fn number_from_null() {
    assert_eq!(deobfuscate("var x = Number(null);"), "var x = 0;");
}

#[test]
fn boolean_from_number_truthy() {
    assert_eq!(deobfuscate("var x = Boolean(1);"), "var x = true;");
}

#[test]
fn boolean_from_number_falsy() {
    assert_eq!(deobfuscate("var x = Boolean(0);"), "var x = false;");
}

#[test]
fn boolean_from_string_truthy() {
    assert_eq!(deobfuscate("var x = Boolean(\"hello\");"), "var x = true;");
}

#[test]
fn boolean_from_string_falsy() {
    assert_eq!(deobfuscate("var x = Boolean(\"\");"), "var x = false;");
}

#[test]
fn boolean_from_null() {
    assert_eq!(deobfuscate("var x = Boolean(null);"), "var x = false;");
}

#[test]
fn atob() {
    assert_eq!(
        deobfuscate("var x = atob(\"SGVsbG8=\");"),
        "var x = \"Hello\";"
    );
}

#[test]
fn chained_string_from_char_code() {
    assert_eq!(
        deobfuscate(
            "var x = String.fromCharCode(116) + String.fromCharCode(101) + String.fromCharCode(115) + String.fromCharCode(116);"
        ),
        "var x = \"test\";"
    );
}
