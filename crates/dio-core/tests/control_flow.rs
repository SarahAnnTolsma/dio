//! Integration tests for the ControlFlowTransformer.

mod common;
use common::deobfuscate;

#[test]
fn if_true() {
    assert_eq!(
        deobfuscate("if (true) { x = 1; } else { x = 2; }"),
        "x = 1;"
    );
}

#[test]
fn if_false_with_else() {
    assert_eq!(
        deobfuscate("if (false) { x = 1; } else { x = 2; }"),
        "x = 2;"
    );
}

#[test]
fn if_false_no_else() {
    assert_eq!(deobfuscate("if (false) { x = 1; }"), "");
}

#[test]
fn ternary_true() {
    assert_eq!(
        deobfuscate("var x = true ? \"yes\" : \"no\"; f(x);"),
        "f(\"yes\");"
    );
}

#[test]
fn ternary_false() {
    assert_eq!(
        deobfuscate("var x = false ? \"yes\" : \"no\"; f(x);"),
        "f(\"no\");"
    );
}

#[test]
fn ternary_numeric_truthy() {
    assert_eq!(
        deobfuscate("var x = 1 ? \"yes\" : \"no\"; f(x);"),
        "f(\"yes\");"
    );
}

#[test]
fn ternary_numeric_falsy() {
    assert_eq!(
        deobfuscate("var x = 0 ? \"yes\" : \"no\"; f(x);"),
        "f(\"no\");"
    );
}

#[test]
fn ternary_empty_string_falsy() {
    assert_eq!(
        deobfuscate("var x = \"\" ? \"yes\" : \"no\"; f(x);"),
        "f(\"no\");"
    );
}

#[test]
fn ternary_nonempty_string_truthy() {
    assert_eq!(
        deobfuscate("var x = \"hi\" ? \"yes\" : \"no\"; f(x);"),
        "f(\"yes\");"
    );
}

#[test]
fn ternary_null_falsy() {
    assert_eq!(
        deobfuscate("var x = null ? \"yes\" : \"no\"; f(x);"),
        "f(\"no\");"
    );
}
