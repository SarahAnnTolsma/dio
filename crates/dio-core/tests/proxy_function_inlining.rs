//! Integration tests for the ProxyFunctionInliningTransformer.

mod common;
use common::deobfuscate;

// ---------------------------------------------------------------------------
// Binary operation proxy
// ---------------------------------------------------------------------------

#[test]
fn binary_addition_proxy() {
    // Proxy inlining exposes `1 + 2`, which constant folding simplifies to `3`.
    assert_eq!(
        deobfuscate("function _0x1(a, b) { return a + b; } var x = _0x1(1, 2);"),
        "var x = 3;"
    );
}

#[test]
fn binary_subtraction_proxy() {
    assert_eq!(
        deobfuscate("function _0x2(a, b) { return a - b; } var x = _0x2(10, 3);"),
        "var x = 7;"
    );
}

#[test]
fn binary_multiplication_proxy() {
    assert_eq!(
        deobfuscate("function _0x3(a, b) { return a * b; } var x = _0x3(3, 4);"),
        "var x = 12;"
    );
}

#[test]
fn binary_comparison_proxy() {
    assert_eq!(
        deobfuscate("function _0x4(a, b) { return a === b; } var x = _0x4(1, 1);"),
        "var x = true;"
    );
}

// ---------------------------------------------------------------------------
// Identity proxy
// ---------------------------------------------------------------------------

#[test]
fn identity_proxy() {
    assert_eq!(
        deobfuscate("function _0x5(a) { return a; } var x = _0x5(42);"),
        "var x = 42;"
    );
}

#[test]
fn identity_proxy_with_expression() {
    assert_eq!(
        deobfuscate("function _0x5(a) { return a; } var x = _0x5(foo());"),
        "var x = foo();"
    );
}

// ---------------------------------------------------------------------------
// Call forwarding proxy
// ---------------------------------------------------------------------------

#[test]
fn call_forwarding_proxy_single_argument() {
    assert_eq!(
        deobfuscate("function _0x6(fn, a) { return fn(a); } _0x6(alert, \"hi\");"),
        "alert(\"hi\");"
    );
}

#[test]
fn call_forwarding_proxy_multiple_arguments() {
    assert_eq!(
        deobfuscate(
            "function _0x7(fn, a, b) { return fn(a, b); } var x = _0x7(Math.max, 1, 2);"
        ),
        "var x = Math.max(1, 2);"
    );
}

#[test]
fn call_forwarding_proxy_no_extra_arguments() {
    assert_eq!(
        deobfuscate("function _0x8(fn) { return fn(); } _0x8(getTime);"),
        "getTime();"
    );
}

// ---------------------------------------------------------------------------
// Multiple call sites
// ---------------------------------------------------------------------------

#[test]
fn binary_proxy_multiple_call_sites() {
    // Both call sites are inlined and then constant-folded.
    assert_eq!(
        deobfuscate(
            "function _0x1(a, b) { return a + b; } var x = _0x1(1, 2); var y = _0x1(3, 4);"
        ),
        "var x = 3;\nvar y = 7;"
    );
}

// ---------------------------------------------------------------------------
// Combined with constant folding
// ---------------------------------------------------------------------------

#[test]
fn binary_proxy_with_variable_arguments() {
    // Proxy inlining with non-constant arguments preserves the operation.
    assert_eq!(
        deobfuscate("function _0x1(a, b) { return a + b; } var x = _0x1(foo, bar);"),
        "var x = foo + bar;"
    );
}

// ---------------------------------------------------------------------------
// No-op cases: should not be inlined
// ---------------------------------------------------------------------------

#[test]
fn skip_function_with_multiple_statements() {
    assert_eq!(
        deobfuscate(
            "function f(a, b) { console.log(a); return a + b; } var x = f(1, 2);"
        ),
        "function f(a, b) {\n\tconsole.log(a);\n\treturn a + b;\n}\nvar x = f(1, 2);"
    );
}

#[test]
fn skip_function_with_free_variables() {
    // The return expression references `c` which is not a parameter.
    assert_eq!(
        deobfuscate("var c = 10; function f(a, b) { return a + c; } var x = f(1, 2);"),
        "function f(a, b) {\n\treturn a + 10;\n}\nvar x = f(1, 2);"
    );
}
