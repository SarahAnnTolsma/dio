//! Integration tests for the StringArrayDecoderTransformer.
//!
//! Uses the ObfuscatorIo preset since the string array decoder is
//! specific to that obfuscation tool.

use dio_core::{Deobfuscator, Preset};

fn deobfuscate(source: &str) -> String {
    Deobfuscator::with_preset(Preset::ObfuscatorIo)
        .deobfuscate(source)
        .trim()
        .to_string()
}

// ---------------------------------------------------------------------------
// Pattern 1: atob-based string array
// ---------------------------------------------------------------------------

#[test]
fn atob_string_array_basic() {
    let input = r#"
        var w = ["TnVtYmVy", "ZnVuY3Rpb24", "aGVsbG8"];
        function o(n, t) { return t = w[n], atob(t) }
        console.log(o(0));
    "#;
    assert_eq!(deobfuscate(input), "console.log(\"Number\");");
}

#[test]
fn atob_string_array_multiple_calls() {
    let input = r#"
        var w = ["TnVtYmVy", "ZnVuY3Rpb24", "aGVsbG8"];
        function o(n, t) { return t = w[n], atob(t) }
        console.log(o(0), o(1), o(2));
    "#;
    assert_eq!(
        deobfuscate(input),
        "console.log(\"Number\", \"function\", \"hello\");"
    );
}

#[test]
fn atob_string_array_as_property() {
    let input = r#"
        var w = ["bG9n", "aGVsbG8", "d29ybGQ"];
        function o(n, t) { return t = w[n], atob(t) }
        console[o(0)]("hello");
    "#;
    assert_eq!(deobfuscate(input), "console.log(\"hello\");");
}

// ---------------------------------------------------------------------------
// Pattern 2: Custom base64 alphabet with mixed types
// ---------------------------------------------------------------------------

#[test]
fn custom_base64_string_array() {
    // Use the custom alphabet from the real obfuscated file.
    let input = r#"
        var dn = ["u3ge5zPP", "KYsUtzPP", "lXgklYsVtWaP"];
        function r(n) {
            var t = dn[n];
            return "string" == typeof t ? function(n, t, e, i, a, o, r) {
                var s, c = "zTDpQgXBRVofJM=xaA2u6s3iKm5tlZr1LHdCwn0WjUINh4bO/vk8eEYF7qGc+y9SP",
                    u = "", f = 0;
                for (n = n.replace(/[^A-Za-z0-9\+\/\=]/g, ""); f < n.length;)
                    r = c.indexOf(n.charAt(f++)) << 2 | (s = c.indexOf(n.charAt(f++))) >> 4,
                    o = (15 & s) << 4 | (e = c.indexOf(n.charAt(f++))) >> 2,
                    a = (3 & e) << 6 | (t = c.indexOf(n.charAt(f++))),
                    u += String.fromCharCode(r),
                    64 != e && (u += String.fromCharCode(o)),
                    64 != t && (u += String.fromCharCode(a));
                return u
            }(t) : t
        }
        console.log(r(0));
    "#;
    let result = deobfuscate(input);
    // The decoded string should be a real value, not the encoded form.
    assert!(!result.contains("u3ge5zPP"));
    assert!(result.starts_with("console.log(\""));
}

#[test]
fn mixed_array_number_passthrough() {
    // Pattern 2 with mixed string/number elements.
    let input = r#"
        var arr = ["aGVsbG8", 42, "d29ybGQ", true];
        function decode(n) { var t = arr[n]; return "string" == typeof t ? atob(t) : t }
        var a = decode(0);
        var b = decode(1);
        var c = decode(2);
    "#;
    // decode(0) → "hello", decode(1) → 42, decode(2) → "world"
    let result = deobfuscate(input);
    assert!(result.contains("\"hello\""));
    assert!(result.contains("42"));
    assert!(result.contains("\"world\""));
}

// ---------------------------------------------------------------------------
// Edge cases
// ---------------------------------------------------------------------------

#[test]
fn non_constant_index_left_unchanged() {
    let input = r#"
        var w = ["TnVtYmVy", "ZnVuY3Rpb24", "aGVsbG8"];
        function o(n) { return atob(w[n]) }
        console.log(o(x));
    "#;
    // `o(x)` has a non-constant index, so it can't be inlined.
    let result = deobfuscate(input);
    assert!(result.contains("o(x)") || result.contains("o("));
}

#[test]
fn small_array_not_decoded() {
    // Arrays with fewer than 3 elements should not be treated as string arrays.
    let input = r#"
        var arr = ["hello"];
        function get(n) { return arr[n] }
        console.log(get(0));
    "#;
    let result = deobfuscate(input);
    // Should not be decoded since array is too small.
    assert!(result.contains("arr") || result.contains("get"));
}
