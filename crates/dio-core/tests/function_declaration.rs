//! Integration tests for the FunctionDeclarationTransformer.

mod common;
use common::deobfuscate;

#[test]
fn anonymous_function_to_declaration() {
    assert_eq!(
        deobfuscate("var foo = function() { return 1; };"),
        "function foo() {\n    return 1;\n}"
    );
}

#[test]
fn anonymous_function_with_params() {
    assert_eq!(
        deobfuscate("var add = function(a, b) { return a + b; };"),
        "function add(a, b) {\n    return a + b;\n}"
    );
}

#[test]
fn let_declaration() {
    assert_eq!(
        deobfuscate("let greet = function() { return \"hi\"; };"),
        "function greet() {\n    return \"hi\";\n}"
    );
}

#[test]
fn const_declaration() {
    assert_eq!(
        deobfuscate("const greet = function() { return \"hi\"; };"),
        "function greet() {\n    return \"hi\";\n}"
    );
}

#[test]
fn skip_named_function_expression() {
    // Named function expressions should not be converted.
    assert_eq!(
        deobfuscate("var x = function named() { return 1; };"),
        "var x = function named() {\n    return 1;\n};"
    );
}

#[test]
fn skip_reassigned_variable() {
    // If the variable is reassigned, do not convert.
    assert_eq!(
        deobfuscate("var x = function() { return 1; }; x = something;"),
        "var x = function() {\n    return 1;\n};\nx = something;"
    );
}

#[test]
fn skip_non_function_initializer() {
    // Non-function initializers should not be affected.
    assert_eq!(deobfuscate("var x = 42; f(x);"), "f(42);");
}

#[test]
fn function_is_called() {
    assert_eq!(
        deobfuscate("var foo = function() { return 1; }; f(foo());"),
        "function foo() {\n    return 1;\n}\nf(foo());"
    );
}
