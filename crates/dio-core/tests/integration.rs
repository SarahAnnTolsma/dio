//! Integration tests for the deobfuscation pipeline.
//!
//! Each test passes a small snippet of obfuscated JavaScript through
//! `Deobfuscator::new().deobfuscate()` and asserts the cleaned output.

use dio_core::Deobfuscator;

/// Helper: deobfuscate and trim trailing whitespace/newlines for comparison.
fn deobfuscate(source: &str) -> String {
    Deobfuscator::new().deobfuscate(source).trim().to_string()
}

// ---------------------------------------------------------------------------
// Constant folding
// ---------------------------------------------------------------------------

#[test]
fn constant_folding_addition() {
    assert_eq!(deobfuscate("var x = 1 + 2;"), "var x = 3;");
}

#[test]
fn constant_folding_subtraction() {
    assert_eq!(deobfuscate("var x = 10 - 3;"), "var x = 7;");
}

#[test]
fn constant_folding_multiplication() {
    assert_eq!(deobfuscate("var x = 4 * 5;"), "var x = 20;");
}

#[test]
fn constant_folding_division() {
    assert_eq!(deobfuscate("var x = 20 / 4;"), "var x = 5;");
}

#[test]
fn constant_folding_nested_arithmetic() {
    // (2 + 3) * (10 - 4) should fold to 30
    assert_eq!(deobfuscate("var x = (2 + 3) * (10 - 4);"), "var x = 30;");
}

#[test]
fn constant_folding_typeof_literal() {
    assert_eq!(
        deobfuscate("var x = typeof 42;"),
        "var x = \"number\";"
    );
}

#[test]
fn constant_folding_typeof_string() {
    assert_eq!(
        deobfuscate("var x = typeof \"hello\";"),
        "var x = \"string\";"
    );
}

#[test]
fn constant_folding_void_zero() {
    assert_eq!(deobfuscate("var x = void 0;"), "var x = undefined;");
}

// ---------------------------------------------------------------------------
// String concatenation
// ---------------------------------------------------------------------------

#[test]
fn string_concatenation_simple() {
    assert_eq!(
        deobfuscate("var x = \"hello\" + \" \" + \"world\";"),
        "var x = \"hello world\";"
    );
}

#[test]
fn string_concatenation_multi_step() {
    assert_eq!(
        deobfuscate("var x = \"a\" + \"b\" + \"c\" + \"d\";"),
        "var x = \"abcd\";"
    );
}

// ---------------------------------------------------------------------------
// Built-in evaluation
// ---------------------------------------------------------------------------

#[test]
fn builtin_eval_string_from_char_code() {
    assert_eq!(
        deobfuscate("var x = String.fromCharCode(72, 105);"),
        "var x = \"Hi\";"
    );
}

#[test]
fn builtin_eval_parse_int() {
    assert_eq!(deobfuscate("var x = parseInt(\"10\");"), "var x = 10;");
}

#[test]
fn builtin_eval_parse_int_hex() {
    assert_eq!(
        deobfuscate("var x = parseInt(\"ff\", 16);"),
        "var x = 255;"
    );
}

#[test]
fn builtin_eval_parse_float() {
    assert_eq!(
        deobfuscate("var x = parseFloat(\"3.14\");"),
        "var x = 3.14;"
    );
}

// Number.parseInt / Number.parseFloat

#[test]
fn builtin_eval_number_parse_int() {
    assert_eq!(
        deobfuscate("var x = Number.parseInt(\"10\");"),
        "var x = 10;"
    );
}

#[test]
fn builtin_eval_number_parse_int_hex() {
    assert_eq!(
        deobfuscate("var x = Number.parseInt(\"ff\", 16);"),
        "var x = 255;"
    );
}

#[test]
fn builtin_eval_number_parse_float() {
    assert_eq!(
        deobfuscate("var x = Number.parseFloat(\"3.14\");"),
        "var x = 3.14;"
    );
}

// Number() type coercion

#[test]
fn builtin_eval_number_from_string() {
    assert_eq!(deobfuscate("var x = Number(\"42\");"), "var x = 42;");
}

#[test]
fn builtin_eval_number_from_float_string() {
    assert_eq!(deobfuscate("var x = Number(\"3.14\");"), "var x = 3.14;");
}

#[test]
fn builtin_eval_number_from_empty_string() {
    assert_eq!(deobfuscate("var x = Number(\"\");"), "var x = 0;");
}

#[test]
fn builtin_eval_number_from_true() {
    assert_eq!(deobfuscate("var x = Number(true);"), "var x = 1;");
}

#[test]
fn builtin_eval_number_from_false() {
    assert_eq!(deobfuscate("var x = Number(false);"), "var x = 0;");
}

#[test]
fn builtin_eval_number_from_null() {
    assert_eq!(deobfuscate("var x = Number(null);"), "var x = 0;");
}

// Boolean() type coercion

#[test]
fn builtin_eval_boolean_from_number_truthy() {
    assert_eq!(deobfuscate("var x = Boolean(1);"), "var x = true;");
}

#[test]
fn builtin_eval_boolean_from_number_falsy() {
    assert_eq!(deobfuscate("var x = Boolean(0);"), "var x = false;");
}

#[test]
fn builtin_eval_boolean_from_string_truthy() {
    assert_eq!(
        deobfuscate("var x = Boolean(\"hello\");"),
        "var x = true;"
    );
}

#[test]
fn builtin_eval_boolean_from_string_falsy() {
    assert_eq!(deobfuscate("var x = Boolean(\"\");"), "var x = false;");
}

#[test]
fn builtin_eval_boolean_from_null() {
    assert_eq!(deobfuscate("var x = Boolean(null);"), "var x = false;");
}

#[test]
fn builtin_eval_atob() {
    assert_eq!(
        deobfuscate("var x = atob(\"SGVsbG8=\");"),
        "var x = \"Hello\";"
    );
}

// ---------------------------------------------------------------------------
// Control flow simplification
// ---------------------------------------------------------------------------

#[test]
fn control_flow_if_true() {
    assert_eq!(
        deobfuscate("if (true) { x = 1; } else { x = 2; }"),
        "x = 1;"
    );
}

#[test]
fn control_flow_if_false_with_else() {
    assert_eq!(
        deobfuscate("if (false) { x = 1; } else { x = 2; }"),
        "x = 2;"
    );
}

#[test]
fn control_flow_if_false_no_else() {
    // `if (false) { x = 1; }` -> removed entirely
    assert_eq!(deobfuscate("if (false) { x = 1; }"), "");
}

#[test]
fn control_flow_ternary_true() {
    assert_eq!(
        deobfuscate("var x = true ? \"yes\" : \"no\";"),
        "var x = \"yes\";"
    );
}

#[test]
fn control_flow_ternary_false() {
    assert_eq!(
        deobfuscate("var x = false ? \"yes\" : \"no\";"),
        "var x = \"no\";"
    );
}

#[test]
fn control_flow_ternary_numeric_truthy() {
    assert_eq!(
        deobfuscate("var x = 1 ? \"yes\" : \"no\";"),
        "var x = \"yes\";"
    );
}

#[test]
fn control_flow_ternary_numeric_falsy() {
    assert_eq!(
        deobfuscate("var x = 0 ? \"yes\" : \"no\";"),
        "var x = \"no\";"
    );
}

#[test]
fn control_flow_ternary_empty_string_falsy() {
    assert_eq!(
        deobfuscate("var x = \"\" ? \"yes\" : \"no\";"),
        "var x = \"no\";"
    );
}

#[test]
fn control_flow_ternary_nonempty_string_truthy() {
    assert_eq!(
        deobfuscate("var x = \"hi\" ? \"yes\" : \"no\";"),
        "var x = \"yes\";"
    );
}

#[test]
fn control_flow_ternary_null_falsy() {
    assert_eq!(
        deobfuscate("var x = null ? \"yes\" : \"no\";"),
        "var x = \"no\";"
    );
}

// ---------------------------------------------------------------------------
// Block normalization (bare statements -> block statements)
// ---------------------------------------------------------------------------

#[test]
fn block_normalization_if() {
    assert_eq!(
        deobfuscate("if (x) foo();"),
        "if (x) {\n\tfoo();\n}"
    );
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
    // `else if` should NOT be wrapped — it's idiomatic.
    assert_eq!(
        deobfuscate("if (x) foo(); else if (y) bar();"),
        "if (x) {\n\tfoo();\n} else if (y) {\n\tbar();\n}"
    );
}

#[test]
fn block_normalization_while() {
    assert_eq!(
        deobfuscate("while (x) foo();"),
        "while (x) {\n\tfoo();\n}"
    );
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
    // Already has blocks — should pass through unchanged.
    assert_eq!(
        deobfuscate("if (x) { foo(); } else { bar(); }"),
        "if (x) {\n\tfoo();\n} else {\n\tbar();\n}"
    );
}

// ---------------------------------------------------------------------------
// Ternary-to-if conversion (standalone ternary expressions)
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
fn ternary_to_if_does_not_affect_value_position() {
    // Ternary used as a value should NOT be converted.
    assert_eq!(
        deobfuscate("var x = a ? b : c;"),
        "var x = a ? b : c;"
    );
}

#[test]
fn ternary_to_if_with_constant_condition_simplifies() {
    // Ternary with constant condition: first converts to if/else, then
    // control flow simplifies the if(true) away.
    assert_eq!(
        deobfuscate("true ? y() : z();"),
        "y();"
    );
}

// ---------------------------------------------------------------------------
// Comma (sequence) expression simplification
// ---------------------------------------------------------------------------

#[test]
fn comma_expression_simplification() {
    assert_eq!(deobfuscate("var x = (1, 2, 3);"), "var x = 3;");
}

#[test]
fn comma_expression_with_identifiers() {
    // Identifiers are considered side-effect-free by the comma transformer
    assert_eq!(deobfuscate("var a = 1; var x = (a, 2, 3);"), "var a = 1;\nvar x = 3;");
}

// ---------------------------------------------------------------------------
// Sequence expression hoisting (return / if with comma expressions)
// ---------------------------------------------------------------------------

#[test]
fn sequence_return_hoists_leading_expressions() {
    assert_eq!(
        deobfuscate("function f() { return (a(), b(), c()); }"),
        "function f() {\n\ta();\n\tb();\n\treturn c();\n}"
    );
}

#[test]
fn sequence_return_two_expressions() {
    assert_eq!(
        deobfuscate("function f() { return (a(), b()); }"),
        "function f() {\n\ta();\n\treturn b();\n}"
    );
}

#[test]
fn sequence_if_hoists_leading_expressions() {
    assert_eq!(
        deobfuscate("if (a(), b(), c) { x(); }"),
        "a();\nb();\nif (c) {\n\tx();\n}"
    );
}

#[test]
fn sequence_if_else_hoists_leading_expressions() {
    assert_eq!(
        deobfuscate("if (a(), b()) { x(); } else { y(); }"),
        "a();\nif (b()) {\n\tx();\n} else {\n\ty();\n}"
    );
}

#[test]
fn sequence_return_single_not_affected() {
    // Single expression return should not be changed.
    assert_eq!(
        deobfuscate("function f() { return x(); }"),
        "function f() {\n\treturn x();\n}"
    );
}

// ---------------------------------------------------------------------------
// Member expression simplification (computed -> dot notation)
// ---------------------------------------------------------------------------

#[test]
fn member_expression_computed_to_dot() {
    assert_eq!(
        deobfuscate("obj[\"property\"];"),
        "obj.property;"
    );
}

#[test]
fn member_expression_keeps_invalid_identifier() {
    // `obj["hello world"]` should NOT be converted — not a valid identifier
    assert_eq!(
        deobfuscate("obj[\"hello world\"];"),
        "obj[\"hello world\"];"
    );
}

#[test]
fn member_expression_keeps_reserved_word() {
    // `obj["class"]` should NOT be converted — reserved word
    assert_eq!(
        deobfuscate("obj[\"class\"];"),
        "obj[\"class\"];"
    );
}

#[test]
fn member_expression_numeric_key() {
    // `obj["0"]` starts with a digit — not a valid JS identifier
    assert_eq!(
        deobfuscate("obj[\"0\"];"),
        "obj[\"0\"];"
    );
}

// ---------------------------------------------------------------------------
// Dead code elimination
// ---------------------------------------------------------------------------

#[test]
fn dead_code_after_return() {
    assert_eq!(
        deobfuscate("function f() { return 1; var x = 2; x + 3; }"),
        "function f() {\n\treturn 1;\n}"
    );
}

#[test]
fn dead_code_after_throw() {
    assert_eq!(
        deobfuscate("function f() { throw new Error(); var x = 2; }"),
        "function f() {\n\tthrow new Error();\n}"
    );
}

// ---------------------------------------------------------------------------
// Combined transforms (multi-layer deobfuscation)
// ---------------------------------------------------------------------------

#[test]
fn combined_fold_then_control_flow() {
    // First fold 1 + 1 -> 2, then 2 is truthy so ternary simplifies.
    assert_eq!(
        deobfuscate("var x = (1 + 1) ? \"yes\" : \"no\";"),
        "var x = \"yes\";"
    );
}

#[test]
fn combined_string_concat_and_builtin() {
    // atob decodes, then string concat joins.
    assert_eq!(
        deobfuscate("var x = atob(\"SGVs\") + atob(\"bG8=\");"),
        "var x = \"Hello\";"
    );
}

#[test]
fn combined_comma_and_fold() {
    // Comma simplifies to the last element, which is then folded.
    assert_eq!(
        deobfuscate("var x = (0, 1 + 2);"),
        "var x = 3;"
    );
}

#[test]
fn combined_nested_ternaries() {
    // Inner ternary resolves first, then outer.
    assert_eq!(
        deobfuscate("var x = true ? (false ? 1 : 2) : 3;"),
        "var x = 2;"
    );
}

#[test]
fn combined_dead_code_after_constant_if() {
    // `if (true)` keeps the consequent; dead code after return is removed.
    assert_eq!(
        deobfuscate(
            "function f() { if (true) { return 1; } else { return 2; } var x = 3; }"
        ),
        "function f() {\n\treturn 1;\n}"
    );
}

#[test]
fn combined_member_and_string() {
    // Computed member with string concat key: `obj["hel" + "lo"]` -> `obj.hello`
    assert_eq!(
        deobfuscate("obj[\"hel\" + \"lo\"];"),
        "obj.hello;"
    );
}

#[test]
fn combined_from_char_code_and_member() {
    // `String.fromCharCode` evaluates, then result used elsewhere.
    assert_eq!(
        deobfuscate("var greeting = String.fromCharCode(72, 101, 108, 108, 111);"),
        "var greeting = \"Hello\";"
    );
}

// ---------------------------------------------------------------------------
// Realistic obfuscation patterns
// ---------------------------------------------------------------------------

#[test]
fn realistic_hex_arithmetic() {
    // Obfuscators often use hex literals — constant folding should handle them.
    assert_eq!(deobfuscate("var x = 0xa + 0x14;"), "var x = 30;");
}

#[test]
fn realistic_nested_comma_with_side_effect_free_leading() {
    assert_eq!(
        deobfuscate("var x = (0, 0, 0, 42);"),
        "var x = 42;"
    );
}

#[test]
fn realistic_chained_string_from_char_code() {
    // Common pattern: building strings character by character.
    assert_eq!(
        deobfuscate(
            "var x = String.fromCharCode(116) + String.fromCharCode(101) + String.fromCharCode(115) + String.fromCharCode(116);"
        ),
        "var x = \"test\";"
    );
}

#[test]
fn passthrough_unobfuscated_code() {
    // Clean code should pass through unchanged (modulo formatting).
    let source = "function add(a, b) {\n\treturn a + b;\n}\n";
    assert_eq!(deobfuscate(source), "function add(a, b) {\n\treturn a + b;\n}");
}
