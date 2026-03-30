//! Integration tests for the DeadCodeTransformer.

mod common;
use common::deobfuscate;

#[test]
fn after_return() {
    assert_eq!(
        deobfuscate("function f() { return 1; var x = 2; x + 3; }"),
        "function f() {\n\treturn 1;\n}"
    );
}

#[test]
fn after_throw() {
    assert_eq!(
        deobfuscate("function f() { throw new Error(); var x = 2; }"),
        "function f() {\n\tthrow new Error();\n}"
    );
}

#[test]
fn combined_with_constant_if() {
    assert_eq!(
        deobfuscate("function f() { if (true) { return 1; } else { return 2; } var x = 3; }"),
        "function f() {\n\treturn 1;\n}"
    );
}
