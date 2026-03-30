//! Integration tests for simplification transformers:
//! BlockNormalization, Comma, TernaryToIf, LogicalToIf, SequenceStatement, Member.

mod common;
use common::deobfuscate;

// ---------------------------------------------------------------------------
// Block normalization
// ---------------------------------------------------------------------------

#[test]
fn block_normalization_if() {
    assert_eq!(deobfuscate("if (x) foo();"), "if (x) {\n    foo();\n}");
}

#[test]
fn block_normalization_if_else() {
    assert_eq!(
        deobfuscate("if (x) foo(); else bar();"),
        "if (x) {\n    foo();\n} else {\n    bar();\n}"
    );
}

#[test]
fn block_normalization_else_if_preserved() {
    assert_eq!(
        deobfuscate("if (x) foo(); else if (y) bar();"),
        "if (x) {\n    foo();\n} else if (y) {\n    bar();\n}"
    );
}

#[test]
fn block_normalization_while() {
    assert_eq!(deobfuscate("while (x) foo();"), "while (x) {\n    foo();\n}");
}

#[test]
fn block_normalization_for() {
    assert_eq!(
        deobfuscate("for (var i = 0; i < 10; i++) foo();"),
        "for (var i = 0; i < 10; i++) {\n    foo();\n}"
    );
}

#[test]
fn block_normalization_do_while() {
    assert_eq!(
        deobfuscate("do foo(); while (x);"),
        "do {\n    foo();\n} while (x);"
    );
}

#[test]
fn block_normalization_for_in() {
    assert_eq!(
        deobfuscate("for (var k in obj) foo();"),
        "for (var k in obj) {\n    foo();\n}"
    );
}

#[test]
fn block_normalization_for_of() {
    assert_eq!(
        deobfuscate("for (var v of arr) foo();"),
        "for (var v of arr) {\n    foo();\n}"
    );
}

#[test]
fn block_normalization_already_blocked() {
    assert_eq!(
        deobfuscate("if (x) { foo(); } else { bar(); }"),
        "if (x) {\n    foo();\n} else {\n    bar();\n}"
    );
}

// ---------------------------------------------------------------------------
// Ternary to if
// ---------------------------------------------------------------------------

#[test]
fn ternary_to_if_simple() {
    assert_eq!(
        deobfuscate("x ? y() : z();"),
        "if (x) {\n    y();\n} else {\n    z();\n}"
    );
}

#[test]
fn ternary_to_if_assignments() {
    assert_eq!(
        deobfuscate("condition ? a = 1 : a = 2;"),
        "if (condition) {\n    a = 1;\n} else {\n    a = 2;\n}"
    );
}

#[test]
fn ternary_to_if_not_in_value_position() {
    assert_eq!(deobfuscate("var x = a ? b : c;"), "var x = a ? b : c;");
}

#[test]
fn ternary_to_if_with_constant_condition() {
    assert_eq!(deobfuscate("true ? y() : z();"), "y();");
}

// ---------------------------------------------------------------------------
// Logical to if
// ---------------------------------------------------------------------------

#[test]
fn logical_and_to_if() {
    assert_eq!(deobfuscate("x && y();"), "if (x) {\n    y();\n}");
}

#[test]
fn logical_or_to_if() {
    assert_eq!(deobfuscate("x || y();"), "if (!x) {\n    y();\n}");
}

#[test]
fn logical_and_not_in_value_position() {
    assert_eq!(deobfuscate("var z = x && y;"), "var z = x && y;");
}

#[test]
fn logical_and_with_constant_condition() {
    assert_eq!(
        deobfuscate("true && console.log(\"hi\");"),
        "console.log(\"hi\");"
    );
}

// ---------------------------------------------------------------------------
// Comma expression
// ---------------------------------------------------------------------------

#[test]
fn comma_simplification() {
    assert_eq!(deobfuscate("var x = (1, 2, 3); f(x);"), "var x = 3;\nf(x);");
}

#[test]
fn comma_with_identifiers() {
    assert_eq!(deobfuscate("var a = 1; var x = (a, 2, 3); f(x);"), "var x = 3;\nf(x);");
}

#[test]
fn comma_nested() {
    assert_eq!(deobfuscate("var x = (0, 0, 0, 42); f(x);"), "var x = 42;\nf(x);");
}

// ---------------------------------------------------------------------------
// Sequence statement hoisting
// ---------------------------------------------------------------------------

#[test]
fn sequence_return() {
    assert_eq!(
        deobfuscate("function f() { return (a(), b(), c()); }"),
        "function f() {\n    a();\n    b();\n    return c();\n}"
    );
}

#[test]
fn sequence_return_two() {
    assert_eq!(
        deobfuscate("function f() { return (a(), b()); }"),
        "function f() {\n    a();\n    return b();\n}"
    );
}

#[test]
fn sequence_if() {
    assert_eq!(
        deobfuscate("if (a(), b(), c) { x(); }"),
        "a();\nb();\nif (c) {\n    x();\n}"
    );
}

#[test]
fn sequence_if_else() {
    assert_eq!(
        deobfuscate("if (a(), b()) { x(); } else { y(); }"),
        "a();\nif (b()) {\n    x();\n} else {\n    y();\n}"
    );
}

#[test]
fn sequence_return_single_not_affected() {
    assert_eq!(
        deobfuscate("function f() { return x(); }"),
        "function f() {\n    return x();\n}"
    );
}

#[test]
fn sequence_while() {
    assert_eq!(
        deobfuscate("while ((a(), b(), c)) { x(); }"),
        "a();\nb();\nwhile (c) {\n    x();\n}"
    );
}

#[test]
fn sequence_throw() {
    assert_eq!(
        deobfuscate("throw (a(), b(), c);"),
        "a();\nb();\nthrow c;"
    );
}

#[test]
fn sequence_switch() {
    assert_eq!(
        deobfuscate("switch ((a(), b(), x)) { case 1: break; }"),
        "a();\nb();\nswitch (x) {\n    case 1: break;\n}"
    );
}

#[test]
fn sequence_for_test() {
    assert_eq!(
        deobfuscate("for (; (a(), b(), c); ) { x(); }"),
        "a();\nb();\nfor (; c;) {\n    x();\n}"
    );
}

// ---------------------------------------------------------------------------
// Sequence expression statement
// ---------------------------------------------------------------------------

#[test]
fn sequence_expression_statement() {
    assert_eq!(
        deobfuscate("(a(), b(), c());"),
        "a();\nb();\nc();"
    );
}

#[test]
fn sequence_expression_statement_two() {
    assert_eq!(deobfuscate("(a(), b());"), "a();\nb();");
}

#[test]
fn sequence_expression_statement_single_not_affected() {
    assert_eq!(deobfuscate("a();"), "a();");
}

// ---------------------------------------------------------------------------
// Variable declaration splitting
// ---------------------------------------------------------------------------

#[test]
fn variable_declaration_split_var() {
    assert_eq!(
        deobfuscate("var a = 1, b = 2, c = 3; f(a, b, c);"),
        "f(1, 2, 3);"
    );
}

#[test]
fn variable_declaration_split_let() {
    assert_eq!(
        deobfuscate("let a = 1, b = 2; f(a, b);"),
        "f(1, 2);"
    );
}

#[test]
fn variable_declaration_split_const() {
    assert_eq!(
        deobfuscate("const a = 1, b = 2; f(a, b);"),
        "f(1, 2);"
    );
}

#[test]
fn variable_declaration_single_unchanged() {
    assert_eq!(deobfuscate("var x = 1; f(x);"), "f(1);");
}

#[test]
fn variable_declaration_split_no_init() {
    // No initializers, and no references — variables are pruned.
    assert_eq!(deobfuscate("var a, b, c;"), "");
}

#[test]
fn variable_declaration_split_mixed_init() {
    assert_eq!(
        deobfuscate("var a = 1, b, c = 3; f(a, c);"),
        "f(1, 3);"
    );
}

// ---------------------------------------------------------------------------
// Member expression
// ---------------------------------------------------------------------------

#[test]
fn member_computed_to_dot() {
    assert_eq!(deobfuscate("obj[\"property\"];"), "obj.property;");
}

#[test]
fn member_keeps_invalid_identifier() {
    assert_eq!(deobfuscate("obj[\"hello world\"];"), "obj[\"hello world\"];");
}

#[test]
fn member_keeps_reserved_word() {
    assert_eq!(deobfuscate("obj[\"class\"];"), "obj[\"class\"];");
}

#[test]
fn member_numeric_key() {
    assert_eq!(deobfuscate("obj[\"0\"];"), "obj[\"0\"];");
}
