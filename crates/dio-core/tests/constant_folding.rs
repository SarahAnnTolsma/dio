//! Integration tests for the ConstantFoldingTransformer.

mod common;
use common::deobfuscate;

#[test]
fn addition() {
    assert_eq!(deobfuscate("var x = 1 + 2; f(x);"), "f(3);");
}

#[test]
fn subtraction() {
    assert_eq!(deobfuscate("var x = 10 - 3; f(x);"), "f(7);");
}

#[test]
fn multiplication() {
    assert_eq!(deobfuscate("var x = 4 * 5; f(x);"), "f(20);");
}

#[test]
fn division() {
    assert_eq!(deobfuscate("var x = 20 / 4; f(x);"), "f(5);");
}

#[test]
fn nested_arithmetic() {
    assert_eq!(deobfuscate("var x = (2 + 3) * (10 - 4); f(x);"), "f(30);");
}

#[test]
fn typeof_literal() {
    assert_eq!(deobfuscate("var x = typeof 42; f(x);"), "f(\"number\");");
}

#[test]
fn typeof_string() {
    assert_eq!(
        deobfuscate("var x = typeof \"hello\"; f(x);"),
        "f(\"string\");"
    );
}

#[test]
fn void_zero() {
    // void 0 → undefined, but undefined is not a literal the pruner removes
    assert_eq!(deobfuscate("var x = void 0; f(x);"), "f(undefined);");
}

#[test]
fn hex_arithmetic() {
    assert_eq!(deobfuscate("var x = 0xa + 0x14; f(x);"), "f(30);");
}

// ---------------------------------------------------------------------------
// JSFuck / type coercion patterns
// ---------------------------------------------------------------------------

#[test]
fn jsfuck_not_array_is_false() {
    assert_eq!(deobfuscate("var x = ![]; f(x);"), "f(false);");
}

#[test]
fn jsfuck_double_not_array_is_true() {
    assert_eq!(deobfuscate("var x = !![]; f(x);"), "f(true);");
}

#[test]
fn jsfuck_plus_empty_array_is_zero() {
    assert_eq!(deobfuscate("var x = +[]; f(x);"), "f(0);");
}

#[test]
fn jsfuck_not_plus_empty_array_is_true() {
    assert_eq!(deobfuscate("var x = !+[]; f(x);"), "f(true);");
}

#[test]
fn jsfuck_plus_double_not_array_is_one() {
    assert_eq!(deobfuscate("var x = +!![]; f(x);"), "f(1);");
}

#[test]
fn jsfuck_not_zero_is_true() {
    assert_eq!(deobfuscate("var x = !0; f(x);"), "f(true);");
}

#[test]
fn jsfuck_not_one_is_false() {
    assert_eq!(deobfuscate("var x = !1; f(x);"), "f(false);");
}

#[test]
fn jsfuck_not_empty_string_is_true() {
    assert_eq!(deobfuscate("var x = !\"\"; f(x);"), "f(true);");
}

#[test]
fn jsfuck_not_nonempty_string_is_false() {
    assert_eq!(deobfuscate("var x = !\"hello\"; f(x);"), "f(false);");
}

#[test]
fn jsfuck_not_null_is_true() {
    assert_eq!(deobfuscate("var x = !null; f(x);"), "f(true);");
}

#[test]
fn jsfuck_plus_true_is_one() {
    assert_eq!(deobfuscate("var x = +true; f(x);"), "f(1);");
}

#[test]
fn jsfuck_plus_false_is_zero() {
    assert_eq!(deobfuscate("var x = +false; f(x);"), "f(0);");
}

#[test]
fn jsfuck_plus_null_is_zero() {
    assert_eq!(deobfuscate("var x = +null; f(x);"), "f(0);");
}

#[test]
fn jsfuck_plus_numeric_string() {
    assert_eq!(deobfuscate("var x = +\"42\"; f(x);"), "f(42);");
}

#[test]
fn jsfuck_plus_empty_string_is_zero() {
    assert_eq!(deobfuscate("var x = +\"\"; f(x);"), "f(0);");
}

#[test]
fn jsfuck_not_object_is_false() {
    assert_eq!(deobfuscate("var x = !{}; f(x);"), "f(false);");
}

#[test]
fn jsfuck_combined_addition() {
    assert_eq!(deobfuscate("var x = +!![] + +!![]; f(x);"), "f(2);");
}

#[test]
fn jsfuck_triple_addition() {
    assert_eq!(deobfuscate("var x = +!![] + +!![] + +!![]; f(x);"), "f(3);");
}
