//! Integration tests for the ConstantFoldingTransformer.

mod common;
use common::deobfuscate;

#[test]
fn addition() {
    assert_eq!(deobfuscate("var x = 1 + 2;"), "var x = 3;");
}

#[test]
fn subtraction() {
    assert_eq!(deobfuscate("var x = 10 - 3;"), "var x = 7;");
}

#[test]
fn multiplication() {
    assert_eq!(deobfuscate("var x = 4 * 5;"), "var x = 20;");
}

#[test]
fn division() {
    assert_eq!(deobfuscate("var x = 20 / 4;"), "var x = 5;");
}

#[test]
fn nested_arithmetic() {
    assert_eq!(deobfuscate("var x = (2 + 3) * (10 - 4);"), "var x = 30;");
}

#[test]
fn typeof_literal() {
    assert_eq!(deobfuscate("var x = typeof 42;"), "var x = \"number\";");
}

#[test]
fn typeof_string() {
    assert_eq!(
        deobfuscate("var x = typeof \"hello\";"),
        "var x = \"string\";"
    );
}

#[test]
fn void_zero() {
    assert_eq!(deobfuscate("var x = void 0;"), "var x = undefined;");
}

#[test]
fn hex_arithmetic() {
    assert_eq!(deobfuscate("var x = 0xa + 0x14;"), "var x = 30;");
}

// ---------------------------------------------------------------------------
// JSFuck / type coercion patterns
// ---------------------------------------------------------------------------

#[test]
fn jsfuck_not_array_is_false() {
    assert_eq!(deobfuscate("var x = ![];"), "var x = false;");
}

#[test]
fn jsfuck_double_not_array_is_true() {
    assert_eq!(deobfuscate("var x = !![];"), "var x = true;");
}

#[test]
fn jsfuck_plus_empty_array_is_zero() {
    assert_eq!(deobfuscate("var x = +[];"), "var x = 0;");
}

#[test]
fn jsfuck_not_plus_empty_array_is_true() {
    assert_eq!(deobfuscate("var x = !+[];"), "var x = true;");
}

#[test]
fn jsfuck_plus_double_not_array_is_one() {
    assert_eq!(deobfuscate("var x = +!![];"), "var x = 1;");
}

#[test]
fn jsfuck_not_zero_is_true() {
    assert_eq!(deobfuscate("var x = !0;"), "var x = true;");
}

#[test]
fn jsfuck_not_one_is_false() {
    assert_eq!(deobfuscate("var x = !1;"), "var x = false;");
}

#[test]
fn jsfuck_not_empty_string_is_true() {
    assert_eq!(deobfuscate("var x = !\"\";"), "var x = true;");
}

#[test]
fn jsfuck_not_nonempty_string_is_false() {
    assert_eq!(deobfuscate("var x = !\"hello\";"), "var x = false;");
}

#[test]
fn jsfuck_not_null_is_true() {
    assert_eq!(deobfuscate("var x = !null;"), "var x = true;");
}

#[test]
fn jsfuck_plus_true_is_one() {
    assert_eq!(deobfuscate("var x = +true;"), "var x = 1;");
}

#[test]
fn jsfuck_plus_false_is_zero() {
    assert_eq!(deobfuscate("var x = +false;"), "var x = 0;");
}

#[test]
fn jsfuck_plus_null_is_zero() {
    assert_eq!(deobfuscate("var x = +null;"), "var x = 0;");
}

#[test]
fn jsfuck_plus_numeric_string() {
    assert_eq!(deobfuscate("var x = +\"42\";"), "var x = 42;");
}

#[test]
fn jsfuck_plus_empty_string_is_zero() {
    assert_eq!(deobfuscate("var x = +\"\";"), "var x = 0;");
}

#[test]
fn jsfuck_not_object_is_false() {
    assert_eq!(deobfuscate("var x = !{};"), "var x = false;");
}

#[test]
fn jsfuck_combined_addition() {
    assert_eq!(deobfuscate("var x = +!![] + +!![];"), "var x = 2;");
}

#[test]
fn jsfuck_triple_addition() {
    assert_eq!(deobfuscate("var x = +!![] + +!![] + +!![];"), "var x = 3;");
}
