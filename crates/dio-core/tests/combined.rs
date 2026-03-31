//! Integration tests for combined multi-transformer interactions.

mod common;
use common::deobfuscate;

#[test]
fn fold_then_control_flow() {
    assert_eq!(
        deobfuscate("var x = (1 + 1) ? \"yes\" : \"no\"; f(x);"),
        "f(\"yes\");"
    );
}

#[test]
fn string_concat_and_builtin() {
    assert_eq!(
        deobfuscate("var x = atob(\"SGVs\") + atob(\"bG8=\"); f(x);"),
        "f(\"Hello\");"
    );
}

#[test]
fn comma_and_fold() {
    assert_eq!(
        deobfuscate("var x = (0, 1 + 2); f(x);"),
        "var x = 3;\nf(x);"
    );
}

#[test]
fn nested_ternaries() {
    assert_eq!(
        deobfuscate("var x = true ? (false ? 1 : 2) : 3; f(x);"),
        "var x = 2;\nf(x);"
    );
}

#[test]
fn member_and_string_concat() {
    assert_eq!(deobfuscate("obj[\"hel\" + \"lo\"];"), "obj.hello;");
}

#[test]
fn from_char_code_and_member() {
    assert_eq!(
        deobfuscate("var greeting = String.fromCharCode(72, 101, 108, 108, 111); f(greeting);"),
        "f(\"Hello\");"
    );
}

#[test]
fn sequence_in_return_with_folding() {
    assert_eq!(
        deobfuscate("function f() { return (1 + 2, 3 + 4, 5 + 6); }"),
        "function f() {\n    return 11;\n}"
    );
}

#[test]
fn passthrough_unobfuscated_code() {
    let source = "function add(a, b) {\n    return a + b;\n}\n";
    assert_eq!(
        deobfuscate(source),
        "function add(a, b) {\n    return a + b;\n}"
    );
}
