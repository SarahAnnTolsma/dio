//! Integration tests for the SetTimeoutUnwrapTransformer.

use dio_core::{Deobfuscator, Preset};

fn deobfuscate(source: &str) -> String {
    Deobfuscator::with_preset(Preset::DataDome)
        .deobfuscate(source)
        .trim()
        .to_string()
}

#[test]
fn unwrap_numeric_assignment() {
    assert_eq!(
        deobfuscate("setTimeout(function() { p = -418; }, 0); f(p);"),
        "p = -418;\nf(p);"
    );
}

#[test]
fn unwrap_positive_numeric() {
    assert_eq!(
        deobfuscate("setTimeout(function() { x = 42; }, 0); f(x);"),
        "x = 42;\nf(x);"
    );
}

#[test]
fn unwrap_string_assignment() {
    assert_eq!(
        deobfuscate("setTimeout(function() { s = \"hello\"; }, 0); f(s);"),
        "s = \"hello\";\nf(s);"
    );
}

#[test]
fn unwrap_boolean_assignment() {
    assert_eq!(
        deobfuscate("setTimeout(function() { b = true; }, 0); f(b);"),
        "b = true;\nf(b);"
    );
}

#[test]
fn unwrap_null_assignment() {
    assert_eq!(
        deobfuscate("setTimeout(function() { n = null; }, 0); f(n);"),
        "n = null;\nf(n);"
    );
}

#[test]
fn skip_nonzero_delay() {
    // Non-zero delay should not be unwrapped.
    assert_eq!(
        deobfuscate("setTimeout(function() { x = 1; }, 100); f(x);"),
        "setTimeout(function() {\n    x = 1;\n}, 100);\nf(x);"
    );
}

#[test]
fn skip_multiple_statements() {
    // Multiple statements in the callback should not be unwrapped.
    assert_eq!(
        deobfuscate("setTimeout(function() { x = 1; y = 2; }, 0); f(x);"),
        "setTimeout(function() {\n    x = 1;\n    y = 2;\n}, 0);\nf(x);"
    );
}

#[test]
fn skip_non_literal_rhs() {
    // Non-literal right-hand side should not be unwrapped.
    assert_eq!(
        deobfuscate("setTimeout(function() { x = foo(); }, 0); f(x);"),
        "setTimeout(function() {\n    x = foo();\n}, 0);\nf(x);"
    );
}

#[test]
fn multiple_unwraps() {
    assert_eq!(
        deobfuscate(
            "setTimeout(function() { a = 1; }, 0); setTimeout(function() { b = 2; }, 0); f(a, b);"
        ),
        "a = 1;\nb = 2;\nf(a, b);"
    );
}
