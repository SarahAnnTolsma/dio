//! Integration tests for simplification transformers:
//! BlockNormalization, Comma, TernaryToIf, LogicalToIf, SequenceStatement, Member.

mod common;
use common::deobfuscate;

// ---------------------------------------------------------------------------
// Block normalization
// ---------------------------------------------------------------------------

#[test]
fn block_normalization_if() {
    assert_eq!(deobfuscate("if (x) foo();"), "if (x) {\n\tfoo();\n}");
}

#[test]
fn block_normalization_if_else() {
    assert_eq!(
        deobfuscate("if (x) foo(); else bar();"),
        "if (x) {\n\tfoo();\n} else {\n\tbar();\n}"
    );
}

#[test]
fn block_normalization_else_if_preserved() {
    assert_eq!(
        deobfuscate("if (x) foo(); else if (y) bar();"),
        "if (x) {\n\tfoo();\n} else if (y) {\n\tbar();\n}"
    );
}

#[test]
fn block_normalization_while() {
    assert_eq!(deobfuscate("while (x) foo();"), "while (x) {\n\tfoo();\n}");
}

#[test]
fn block_normalization_for() {
    assert_eq!(
        deobfuscate("for (var i = 0; i < 10; i++) foo();"),
        "for (var i = 0; i < 10; i++) {\n\tfoo();\n}"
    );
}

#[test]
fn block_normalization_do_while() {
    assert_eq!(
        deobfuscate("do foo(); while (x);"),
        "do {\n\tfoo();\n} while (x);"
    );
}

#[test]
fn block_normalization_for_in() {
    assert_eq!(
        deobfuscate("for (var k in obj) foo();"),
        "for (var k in obj) {\n\tfoo();\n}"
    );
}

#[test]
fn block_normalization_for_of() {
    assert_eq!(
        deobfuscate("for (var v of arr) foo();"),
        "for (var v of arr) {\n\tfoo();\n}"
    );
}

#[test]
fn block_normalization_already_blocked() {
    assert_eq!(
        deobfuscate("if (x) { foo(); } else { bar(); }"),
        "if (x) {\n\tfoo();\n} else {\n\tbar();\n}"
    );
}

// ---------------------------------------------------------------------------
// Ternary to if
// ---------------------------------------------------------------------------

#[test]
fn ternary_to_if_simple() {
    assert_eq!(
        deobfuscate("x ? y() : z();"),
        "if (x) {\n\ty();\n} else {\n\tz();\n}"
    );
}

#[test]
fn ternary_to_if_assignments() {
    assert_eq!(
        deobfuscate("condition ? a = 1 : a = 2;"),
        "if (condition) {\n\ta = 1;\n} else {\n\ta = 2;\n}"
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
    assert_eq!(deobfuscate("x && y();"), "if (x) {\n\ty();\n}");
}

#[test]
fn logical_or_to_if() {
    assert_eq!(deobfuscate("x || y();"), "if (!x) {\n\ty();\n}");
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
    assert_eq!(deobfuscate("var x = (1, 2, 3);"), "var x = 3;");
}

#[test]
fn comma_with_identifiers() {
    // `a` is a constant (1), so it gets inlined into the sequence,
    // then the comma transformer drops the side-effect-free leading values.
    assert_eq!(deobfuscate("var a = 1; var x = (a, 2, 3);"), "var x = 3;");
}

#[test]
fn comma_nested() {
    assert_eq!(deobfuscate("var x = (0, 0, 0, 42);"), "var x = 42;");
}

// ---------------------------------------------------------------------------
// Sequence statement hoisting
// ---------------------------------------------------------------------------

#[test]
fn sequence_return() {
    assert_eq!(
        deobfuscate("function f() { return (a(), b(), c()); }"),
        "function f() {\n\ta();\n\tb();\n\treturn c();\n}"
    );
}

#[test]
fn sequence_return_two() {
    assert_eq!(
        deobfuscate("function f() { return (a(), b()); }"),
        "function f() {\n\ta();\n\treturn b();\n}"
    );
}

#[test]
fn sequence_if() {
    assert_eq!(
        deobfuscate("if (a(), b(), c) { x(); }"),
        "a();\nb();\nif (c) {\n\tx();\n}"
    );
}

#[test]
fn sequence_if_else() {
    assert_eq!(
        deobfuscate("if (a(), b()) { x(); } else { y(); }"),
        "a();\nif (b()) {\n\tx();\n} else {\n\ty();\n}"
    );
}

#[test]
fn sequence_return_single_not_affected() {
    assert_eq!(
        deobfuscate("function f() { return x(); }"),
        "function f() {\n\treturn x();\n}"
    );
}

#[test]
fn sequence_while() {
    assert_eq!(
        deobfuscate("while ((a(), b(), c)) { x(); }"),
        "a();\nb();\nwhile (c) {\n\tx();\n}"
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
        "a();\nb();\nswitch (x) {\n\tcase 1: break;\n}"
    );
}

#[test]
fn sequence_for_test() {
    assert_eq!(
        deobfuscate("for (; (a(), b(), c); ) { x(); }"),
        "a();\nb();\nfor (; c;) {\n\tx();\n}"
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
        deobfuscate("var a = 1, b = 2, c = 3;"),
        "var a = 1;\nvar b = 2;\nvar c = 3;"
    );
}

#[test]
fn variable_declaration_split_let() {
    assert_eq!(
        deobfuscate("let a = 1, b = 2;"),
        "let a = 1;\nlet b = 2;"
    );
}

#[test]
fn variable_declaration_split_const() {
    assert_eq!(
        deobfuscate("const a = 1, b = 2;"),
        "const a = 1;\nconst b = 2;"
    );
}

#[test]
fn variable_declaration_single_unchanged() {
    assert_eq!(deobfuscate("var x = 1;"), "var x = 1;");
}

#[test]
fn variable_declaration_split_no_init() {
    assert_eq!(
        deobfuscate("var a, b, c;"),
        "var a;\nvar b;\nvar c;"
    );
}

#[test]
fn variable_declaration_split_mixed_init() {
    assert_eq!(
        deobfuscate("var a = 1, b, c = 3;"),
        "var a = 1;\nvar b;\nvar c = 3;"
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
