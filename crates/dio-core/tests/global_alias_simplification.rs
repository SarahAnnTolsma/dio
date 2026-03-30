//! Integration tests for the GlobalAliasSimplificationTransformer.

mod common;
use common::deobfuscate;

#[test]
fn window_alias_member_access() {
    assert_eq!(
        deobfuscate("var wn = window; wn.Number(\"42\");"),
        "42;"
    );
}

#[test]
fn window_alias_math_method() {
    assert_eq!(
        deobfuscate("var wn = window; wn.Math.ceil(1.5);"),
        "2;"
    );
}

#[test]
fn window_alias_parse_int() {
    assert_eq!(
        deobfuscate("var wn = window; wn.parseInt(\"10\");"),
        "10;"
    );
}

#[test]
fn window_alias_multiple_references() {
    assert_eq!(
        deobfuscate("var wn = window; wn.Number(\"1\"); wn.Boolean(0);"),
        "1;\nfalse;"
    );
}

#[test]
fn self_alias() {
    assert_eq!(
        deobfuscate("var s = self; s.Number(\"5\");"),
        "5;"
    );
}

#[test]
fn global_this_alias() {
    assert_eq!(
        deobfuscate("var g = globalThis; g.parseInt(\"16\", 16);"),
        "22;"
    );
}

#[test]
fn skip_reassigned_alias() {
    // If the alias is reassigned, we should not simplify.
    assert_eq!(
        deobfuscate("var wn = window; wn = something; wn.foo();"),
        "var wn = window;\nwn = something;\nwn.foo();"
    );
}

#[test]
fn alias_with_arbitrary_property() {
    // Non-builtin properties should still be simplified to bare globals.
    assert_eq!(
        deobfuscate("var wn = window; f(wn.document);"),
        "f(document);"
    );
}

#[test]
fn skip_non_global_initializer() {
    // Variables assigned to non-global objects should not be affected.
    assert_eq!(
        deobfuscate("var obj = something; f(obj.foo);"),
        "var obj = something;\nf(obj.foo);"
    );
}
