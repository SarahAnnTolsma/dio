//! Integration tests for the StringConcatenationTransformer.

mod common;
use common::deobfuscate;

#[test]
fn simple() {
    assert_eq!(
        deobfuscate("var x = \"hello\" + \" \" + \"world\";"),
        "var x = \"hello world\";"
    );
}

#[test]
fn multi_step() {
    assert_eq!(
        deobfuscate("var x = \"a\" + \"b\" + \"c\" + \"d\";"),
        "var x = \"abcd\";"
    );
}
