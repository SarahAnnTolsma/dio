//! Integration tests for the DeadCodeTransformer.

mod common;
use common::deobfuscate;

#[test]
fn after_return() {
    assert_eq!(
        deobfuscate("function f() { return 1; var x = 2; x + 3; }"),
        "function f() {\n    return 1;\n}"
    );
}

#[test]
fn after_throw() {
    assert_eq!(
        deobfuscate("function f() { throw new Error(); var x = 2; }"),
        "function f() {\n    throw new Error();\n}"
    );
}

#[test]
fn combined_with_constant_if() {
    assert_eq!(
        deobfuscate("function f() { if (true) { return 1; } else { return 2; } var x = 3; }"),
        "function f() {\n    return 1;\n}"
    );
}

// -- Side-effect-free expression statement removal --

#[test]
fn remove_numeric_literal_statement() {
    assert_eq!(
        deobfuscate("f(); 3; g();"),
        "f();\ng();"
    );
}

#[test]
fn remove_boolean_literal_statement() {
    assert_eq!(
        deobfuscate("f(); true; false; g();"),
        "f();\ng();"
    );
}

#[test]
fn remove_null_literal_statement() {
    assert_eq!(
        deobfuscate("f(); null; g();"),
        "f();\ng();"
    );
}

#[test]
fn remove_undefined_statement() {
    assert_eq!(
        deobfuscate("f(); undefined; g();"),
        "f();\ng();"
    );
}

#[test]
fn remove_void_zero_statement() {
    assert_eq!(
        deobfuscate("f(); void 0; g();"),
        "f();\ng();"
    );
}

#[test]
fn remove_string_literal_statement() {
    assert_eq!(
        deobfuscate("f(); \"hello\"; g();"),
        "f();\ng();"
    );
}

#[test]
fn preserve_use_strict_directive() {
    assert_eq!(
        deobfuscate("\"use strict\"; f();"),
        "\"use strict\";\nf();"
    );
}

#[test]
fn preserve_use_asm_directive() {
    assert_eq!(
        deobfuscate("\"use asm\"; f();"),
        "\"use asm\";\nf();"
    );
}

#[test]
fn keep_function_call_statement() {
    // Function calls have side effects and must be kept.
    assert_eq!(
        deobfuscate("f(); 42; g();"),
        "f();\ng();"
    );
}

#[test]
fn remove_multiple_side_effect_free() {
    assert_eq!(
        deobfuscate("1; 2; 3; f(); 4; 5;"),
        "f();"
    );
}
