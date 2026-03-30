//! Integration tests for the ConstantInliningTransformer.

mod common;
use common::deobfuscate;

// ---------------------------------------------------------------------------
// Basic inlining
// ---------------------------------------------------------------------------

#[test]
fn inline_numeric_constant() {
    assert_eq!(
        deobfuscate("var x = 5; console.log(x);"),
        "console.log(5);"
    );
}

#[test]
fn inline_string_constant() {
    assert_eq!(
        deobfuscate("var greeting = \"hello\"; console.log(greeting);"),
        "console.log(\"hello\");"
    );
}

#[test]
fn inline_boolean_constant() {
    assert_eq!(
        deobfuscate("var flag = true; if (flag) { x(); }"),
        "x();"
    );
}

#[test]
fn inline_null_constant() {
    assert_eq!(
        deobfuscate("var n = null; console.log(n);"),
        "console.log(null);"
    );
}

// ---------------------------------------------------------------------------
// Multiple references
// ---------------------------------------------------------------------------

#[test]
fn inline_multiple_references() {
    assert_eq!(
        deobfuscate("var x = 42; f(x); g(x);"),
        "f(42);\ng(42);"
    );
}

// ---------------------------------------------------------------------------
// var declarations (not just const)
// ---------------------------------------------------------------------------

#[test]
fn inline_var_declaration() {
    assert_eq!(
        deobfuscate("var x = 10; console.log(x);"),
        "console.log(10);"
    );
}

#[test]
fn inline_let_declaration() {
    assert_eq!(
        deobfuscate("let x = 10; console.log(x);"),
        "console.log(10);"
    );
}

#[test]
fn inline_const_declaration() {
    assert_eq!(
        deobfuscate("const x = 10; console.log(x);"),
        "console.log(10);"
    );
}

// ---------------------------------------------------------------------------
// Skips reassigned variables
// ---------------------------------------------------------------------------

#[test]
fn skip_reassigned_variable() {
    assert_eq!(
        deobfuscate("var x = 5; x = 10; console.log(x);"),
        "var x = 5;\nx = 10;\nconsole.log(x);"
    );
}

// ---------------------------------------------------------------------------
// Skips non-literal initializers
// ---------------------------------------------------------------------------

#[test]
fn skip_non_literal_initializer() {
    assert_eq!(
        deobfuscate("var x = getValue(); console.log(x);"),
        "var x = getValue();\nconsole.log(x);"
    );
}

// ---------------------------------------------------------------------------
// Skips variables with no references (dead code)
// ---------------------------------------------------------------------------

#[test]
fn skip_unreferenced_variable() {
    // Dead code removal is not the constant inliner's job.
    assert_eq!(deobfuscate("var x = 5;"), "var x = 5;");
}

// ---------------------------------------------------------------------------
// Negative numbers
// ---------------------------------------------------------------------------

#[test]
fn inline_negative_number() {
    assert_eq!(
        deobfuscate("var x = -1; console.log(x);"),
        "console.log(-1);"
    );
}

// ---------------------------------------------------------------------------
// Chained inlining
// ---------------------------------------------------------------------------

#[test]
fn chained_inlining() {
    // var a = 1; var b = a; → b is inlined to 1, then a is inlined to 1.
    // But since b's initializer is `a` (not a literal), it won't be inlined
    // in the first pass. After a is inlined, b = 1 which can then be inlined.
    assert_eq!(
        deobfuscate("var a = 1; var b = a; console.log(b);"),
        "console.log(1);"
    );
}

// ---------------------------------------------------------------------------
// Multiple declarations in one statement
// ---------------------------------------------------------------------------

#[test]
fn multiple_declarators_all_inlinable() {
    // Both a and b are inlined, then constant folding simplifies 1 + 2 to 3.
    assert_eq!(
        deobfuscate("var a = 1, b = 2; console.log(a + b);"),
        "console.log(3);"
    );
}

#[test]
fn multiple_declarators_partial_inlinable() {
    // The declaration is split first, then `a` is inlined (not reassigned).
    // `b` is reassigned so it is kept.
    assert_eq!(
        deobfuscate("var a = 1, b = 2; b = 3; console.log(a, b);"),
        "var b = 2;\nb = 3;\nconsole.log(1, b);"
    );
}
