//! Integration tests for the LiteralMethodEvaluationTransformer.

mod common;
use common::deobfuscate;

// ---------------------------------------------------------------------------
// String method calls
// ---------------------------------------------------------------------------

#[test]
fn char_at() {
    assert_eq!(deobfuscate("var x = \"hello\".charAt(0);"), "var x = \"h\";");
}

#[test]
fn char_code_at() {
    assert_eq!(deobfuscate("var x = \"hello\".charCodeAt(0);"), "var x = 104;");
}

#[test]
fn index_of() {
    assert_eq!(deobfuscate("var x = \"hello\".indexOf(\"ll\");"), "var x = 2;");
}

#[test]
fn includes() {
    assert_eq!(deobfuscate("var x = \"hello\".includes(\"ell\");"), "var x = true;");
}

#[test]
fn starts_with() {
    assert_eq!(deobfuscate("var x = \"hello\".startsWith(\"he\");"), "var x = true;");
}

#[test]
fn to_lower_case() {
    assert_eq!(deobfuscate("var x = \"HELLO\".toLowerCase();"), "var x = \"hello\";");
}

#[test]
fn to_upper_case() {
    assert_eq!(deobfuscate("var x = \"hello\".toUpperCase();"), "var x = \"HELLO\";");
}

#[test]
fn trim() {
    assert_eq!(deobfuscate("var x = \"  hello  \".trim();"), "var x = \"hello\";");
}

#[test]
fn slice() {
    assert_eq!(deobfuscate("var x = \"hello\".slice(1, 3);"), "var x = \"el\";");
}

#[test]
fn repeat() {
    assert_eq!(deobfuscate("var x = \"ab\".repeat(3);"), "var x = \"ababab\";");
}

#[test]
fn replace() {
    assert_eq!(deobfuscate("var x = \"hello\".replace(\"l\", \"r\");"), "var x = \"herlo\";");
}

// ---------------------------------------------------------------------------
// Property access on literals
// ---------------------------------------------------------------------------

#[test]
fn string_length() {
    assert_eq!(deobfuscate("var x = \"hello\".length;"), "var x = 5;");
}

#[test]
fn string_index_access() {
    assert_eq!(deobfuscate("var x = \"hello\"[0];"), "var x = \"h\";");
}

#[test]
fn array_length() {
    assert_eq!(deobfuscate("var x = [1, 2, 3].length;"), "var x = 3;");
}

#[test]
fn array_index_access() {
    assert_eq!(deobfuscate("var x = [10, 20, 30][1];"), "var x = 20;");
}
