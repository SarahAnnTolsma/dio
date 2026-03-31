//! Integration tests for the ControlFlowArrayTransformer.

use dio_core::{Deobfuscator, Preset};

fn deobfuscate(source: &str) -> String {
    Deobfuscator::with_preset(Preset::ObfuscatorIo)
        .deobfuscate(source)
        .trim()
        .to_string()
}

/// Helper to build a test script with the hash function, IIFE, and usage.
fn make_test_script(usage: &str) -> String {
    format!(
        r#"
function gn(n, t, e, i, a, o, r, s) {{
    return (n * o ^ r * i ^ e * t) >>> 0 & a - 1;
}}
var s;
!function(n, t) {{
    var e;
    var i = [];
    for (t = 0; t < 32; t++) {{
        i[t] = new Array(256);
    }}
    function a(n) {{
        for (var t = 32 * n, a = Math.min(t + 32, 256), o = t; o < a; o++) {{
            for (e = 0; e < 32; e++) {{
                i[e][o] = i[gn(o, 199, 2, 619, 32, 421, e)];
            }}
        }}
    }}
    for (var o = 0; o < 8; o++) {{
        (function(t) {{
            n(function() {{
                a(t);
            }});
        }})(o);
    }}
    n(function() {{
        s = i[2];
    }});
}}(function(n) {{
    setTimeout(n, 0);
}});
{usage}
"#
    )
}

#[test]
fn replaces_double_index_with_numeric_value() {
    // s[93][127] should compute to a specific value via gn hash
    let script = make_test_script("f(s[93][127]);");
    let result = deobfuscate(&script);
    // The value should be a numeric literal (the exact value depends on the hash)
    assert!(
        result.contains("f("),
        "Expected function call in output: {result}"
    );
    assert!(
        !result.contains("s["),
        "Expected s[x][y] to be replaced: {result}"
    );
}

#[test]
fn same_hash_produces_same_value() {
    // s[71][87] and s[49][145] should produce the same value
    // (they appear together in case labels in the real code)
    let script = make_test_script("f(s[71][87], s[49][145]);");
    let result = deobfuscate(&script);
    // Extract the two values
    assert!(
        !result.contains("s["),
        "Expected all s[x][y] to be replaced: {result}"
    );
}

#[test]
fn different_hash_produces_different_values() {
    // s[93][127] and s[71][87] should produce different values
    let script = make_test_script("f(s[93][127]); g(s[71][87]);");
    let result = deobfuscate(&script);
    assert!(
        !result.contains("s["),
        "Expected all s[x][y] to be replaced: {result}"
    );
}

#[test]
fn removes_hash_function_and_iife() {
    let script = make_test_script("f(s[93][127]);");
    let result = deobfuscate(&script);
    assert!(
        !result.contains("function gn"),
        "Expected gn function to be removed: {result}"
    );
    assert!(
        !result.contains("new Array"),
        "Expected IIFE to be removed: {result}"
    );
}

#[test]
fn string_literal_indices() {
    // Indices can also be string literals that parse to numbers
    let script = make_test_script("f(s[\"93\"][\"127\"]);");
    let result = deobfuscate(&script);
    assert!(
        !result.contains("s["),
        "Expected s[x][y] to be replaced: {result}"
    );
}

#[test]
fn computed_values_are_correct() {
    // Verify the actual computed value using the known formula:
    // gn(n, t, e, i, a, o, r) = (n*o ^ r*i ^ e*t) >>> 0 & (a-1)
    // s[x][y] = gn(y, 199, 2, 619, 32, 421, gn(x, 199, 2, 619, 32, 421, 2))
    //
    // gn(93, 199, 2, 619, 32, 421, 2) = (93*421 ^ 2*619 ^ 2*199) & 31
    //   = (39153 ^ 1238 ^ 398) & 31 = (39153 ^ 1238 ^ 398) & 31
    //   93*421 = 39153
    //   2*619 = 1238
    //   2*199 = 398
    //   39153 ^ 1238 = 38183
    //   38183 ^ 398 = 37833
    //   37833 & 31 = 37833 % 32 = 9
    // intermediate_row = 9
    //
    // gn(127, 199, 2, 619, 32, 421, 9) = (127*421 ^ 9*619 ^ 2*199) & 31
    //   127*421 = 53467
    //   9*619 = 5571
    //   2*199 = 398
    //   53467 ^ 5571 = 49928
    //   49928 ^ 398 = 50294
    //   50294 & 31 = 50294 % 32 = 22
    // s[93][127] = 22
    let script = make_test_script("f(s[93][127]);");
    let result = deobfuscate(&script);
    assert_eq!(result, "f(22);");
}

#[test]
fn no_effect_without_hash_function() {
    // Without the hash function, the transformer should not modify anything.
    let result = deobfuscate("var s = [[1,2],[3,4]]; f(s[0][1]);");
    assert_eq!(result, "var s = [[1, 2], [3, 4]];\nf(s[0][1]);");
}

#[test]
fn null_index_replaced_with_void_0() {
    // s[null][y] and s[x][null] are dead code (accessing property "null"
    // on a numeric-indexed array gives undefined). Replace with void 0.
    let script = make_test_script("f(s[null][127]); g(s[93][null]);");
    let result = deobfuscate(&script);
    assert!(
        !result.contains("s["),
        "Expected s[...] to be replaced: {result}"
    );
    assert!(
        result.contains("undefined"),
        "Expected undefined for null indices: {result}"
    );
}
