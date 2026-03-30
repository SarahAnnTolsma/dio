//! Integration tests for the StringConcatenationTransformer.

mod common;
use common::deobfuscate;

#[test]
fn simple() {
    assert_eq!(
        deobfuscate("var x = \"hello\" + \" \" + \"world\"; f(x);"),
        "f(\"hello world\");"
    );
}

#[test]
fn multi_step() {
    assert_eq!(
        deobfuscate("var x = \"a\" + \"b\" + \"c\" + \"d\"; f(x);"),
        "f(\"abcd\");"
    );
}
