//! Integration tests for the ProxyFunctionInliningTransformer.

mod common;
use common::deobfuscate;

// ---------------------------------------------------------------------------
// Binary operation proxy
// ---------------------------------------------------------------------------

#[test]
fn binary_addition_proxy() {
    assert_eq!(
        deobfuscate("function _0x1(a, b) { return a + b; } f(_0x1(1, 2));"),
        "f(3);"
    );
}

#[test]
fn binary_subtraction_proxy() {
    assert_eq!(
        deobfuscate("function _0x2(a, b) { return a - b; } f(_0x2(10, 3));"),
        "f(7);"
    );
}

#[test]
fn binary_multiplication_proxy() {
    assert_eq!(
        deobfuscate("function _0x3(a, b) { return a * b; } f(_0x3(3, 4));"),
        "f(12);"
    );
}

#[test]
fn binary_comparison_proxy() {
    assert_eq!(
        deobfuscate("function _0x4(a, b) { return a === b; } f(_0x4(1, 1));"),
        "f(true);"
    );
}

// ---------------------------------------------------------------------------
// Identity proxy
// ---------------------------------------------------------------------------

#[test]
fn identity_proxy() {
    assert_eq!(
        deobfuscate("function _0x5(a) { return a; } f(_0x5(42));"),
        "f(42);"
    );
}

#[test]
fn identity_proxy_with_expression() {
    assert_eq!(
        deobfuscate("function _0x5(a) { return a; } var x = _0x5(foo()); f(x);"),
        "var x = foo();\nf(x);"
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
        deobfuscate("function _0x7(fn, a, b) { return fn(a, b); } f(_0x7(Math.max, 1, 2));"),
        "f(2);"
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
    assert_eq!(
        deobfuscate("function _0x1(a, b) { return a + b; } f(_0x1(1, 2)); g(_0x1(3, 4));"),
        "f(3);\ng(7);"
    );
}

// ---------------------------------------------------------------------------
// Combined with constant folding
// ---------------------------------------------------------------------------

#[test]
fn binary_proxy_with_variable_arguments() {
    assert_eq!(
        deobfuscate("function _0x1(a, b) { return a + b; } f(_0x1(foo, bar));"),
        "f(foo + bar);"
    );
}

// ---------------------------------------------------------------------------
// Swapped parameter order (common in obfuscators)
// ---------------------------------------------------------------------------

#[test]
fn binary_proxy_swapped_params() {
    assert_eq!(
        deobfuscate("function c(n, t) { return t + n; } f(c(1, 2));"),
        "f(3);"
    );
}

#[test]
fn binary_proxy_swapped_subtraction() {
    assert_eq!(
        deobfuscate("function l(n, t) { return t - n; } f(l(3, 10));"),
        "f(7);"
    );
}

// ---------------------------------------------------------------------------
// Identity returning non-first parameter
// ---------------------------------------------------------------------------

#[test]
fn identity_returns_second_param() {
    assert_eq!(
        deobfuscate("function u(n, t) { return t; } f(u(1, 42));"),
        "f(42);"
    );
}

#[test]
fn identity_returns_first_param_with_extra() {
    assert_eq!(
        deobfuscate("function P(n, t) { return n; } f(P(42, 1));"),
        "f(42);"
    );
}

// ---------------------------------------------------------------------------
// No-op cases: should not be inlined
// ---------------------------------------------------------------------------

#[test]
fn skip_function_with_multiple_statements() {
    assert_eq!(
        deobfuscate("function f(a, b) { console.log(a); return a + b; } var x = f(1, 2);"),
        "function f(a, b) {\n    console.log(a);\n    return a + b;\n}\nvar x = f(1, 2);"
    );
}

#[test]
fn skip_function_with_free_variables() {
    assert_eq!(
        deobfuscate("var c = 10; function f(a, b) { return a + c; } var x = f(1, 2);"),
        "function f(a, b) {\n    return a + 10;\n}\nvar x = f(1, 2);"
    );
}
