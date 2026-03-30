//! Integration tests for the LiteralMethodEvaluationTransformer.

mod common;
use common::deobfuscate;

// ---------------------------------------------------------------------------
// String method calls
// ---------------------------------------------------------------------------

#[test]
fn char_at() {
    assert_eq!(deobfuscate("var x = \"hello\".charAt(0); f(x);"), "f(\"h\");");
}

#[test]
fn char_code_at() {
    assert_eq!(deobfuscate("var x = \"hello\".charCodeAt(0); f(x);"), "f(104);");
}

#[test]
fn index_of() {
    assert_eq!(deobfuscate("var x = \"hello\".indexOf(\"ll\"); f(x);"), "f(2);");
}

#[test]
fn includes() {
    assert_eq!(deobfuscate("var x = \"hello\".includes(\"ell\"); f(x);"), "f(true);");
}

#[test]
fn starts_with() {
    assert_eq!(deobfuscate("var x = \"hello\".startsWith(\"he\"); f(x);"), "f(true);");
}

#[test]
fn to_lower_case() {
    assert_eq!(deobfuscate("var x = \"HELLO\".toLowerCase(); f(x);"), "f(\"hello\");");
}

#[test]
fn to_upper_case() {
    assert_eq!(deobfuscate("var x = \"hello\".toUpperCase(); f(x);"), "f(\"HELLO\");");
}

#[test]
fn trim() {
    assert_eq!(deobfuscate("var x = \"  hello  \".trim(); f(x);"), "f(\"hello\");");
}

#[test]
fn slice() {
    assert_eq!(deobfuscate("var x = \"hello\".slice(1, 3); f(x);"), "f(\"el\");");
}

#[test]
fn repeat() {
    assert_eq!(deobfuscate("var x = \"ab\".repeat(3); f(x);"), "f(\"ababab\");");
}

#[test]
fn replace() {
    assert_eq!(deobfuscate("var x = \"hello\".replace(\"l\", \"r\"); f(x);"), "f(\"herlo\");");
}

// ---------------------------------------------------------------------------
// Property access on literals
// ---------------------------------------------------------------------------

#[test]
fn string_length() {
    assert_eq!(deobfuscate("var x = \"hello\".length; f(x);"), "f(5);");
}

#[test]
fn string_index_access() {
    assert_eq!(deobfuscate("var x = \"hello\"[0]; f(x);"), "f(\"h\");");
}

#[test]
fn array_length() {
    assert_eq!(deobfuscate("var x = [1, 2, 3].length; f(x);"), "f(3);");
}

#[test]
fn array_index_access() {
    assert_eq!(deobfuscate("var x = [10, 20, 30][1]; f(x);"), "f(20);");
}
