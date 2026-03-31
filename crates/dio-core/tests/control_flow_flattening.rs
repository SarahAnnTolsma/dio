//! Integration tests for the ControlFlowFlatteningTransformer.

use dio_core::{Deobfuscator, Preset};

fn deobfuscate(source: &str) -> String {
    Deobfuscator::with_preset(Preset::ObfuscatorIo)
        .deobfuscate(source)
        .trim()
        .to_string()
}

#[test]
fn linearize_simple_transition() {
    // State 1 executes code, transitions to 2 (exit).
    let result = deobfuscate(
        r#"
        function f() {
            for (var x, s = 1; true;) {
                switch (s) {
                    case 99:
                    case 2: break;
                    case 99:
                    case 1:
                        x = 42;
                        s = 2;
                        continue;
                }
                break;
            }
            return x;
        }
        "#,
    );
    assert!(
        !result.contains("switch"),
        "Expected switch to be removed: {result}"
    );
    // x = 42 gets inlined into `return 42;` by constant inlining.
    assert!(result.contains("42"), "Expected 42 in output: {result}");
}

#[test]
fn linearize_three_states() {
    // State 1 -> 2 -> 3 (exit).
    let result = deobfuscate(
        r#"
        function f() {
            for (var a, b, s = 1; true;) {
                switch (s) {
                    case 99:
                    case 3: break;
                    case 99:
                    case 1:
                        a = 10;
                        s = 2;
                        continue;
                    case 99:
                    case 2:
                        b = a + 5;
                        s = 3;
                        continue;
                }
                break;
            }
            return b;
        }
        "#,
    );
    assert!(
        !result.contains("switch"),
        "Expected switch to be removed: {result}"
    );
}

#[test]
fn linearize_terminal_return() {
    let result = deobfuscate(
        r#"
        function f(n) {
            for (var s = 1; true;) {
                switch (s) {
                    case 99:
                    case 1:
                        return n + 1;
                }
                break;
            }
        }
        "#,
    );
    assert!(
        !result.contains("switch"),
        "Expected switch to be removed: {result}"
    );
    assert!(
        result.contains("return n + 1"),
        "Expected return: {result}"
    );
}

#[test]
fn skip_non_boolean_condition() {
    let result = deobfuscate(
        r#"
        for (var s = 1; x;) {
            switch (s) {
                case 1: s = 2; continue;
                case 2: break;
            }
            break;
        }
        "#,
    );
    assert!(
        result.contains("switch"),
        "Expected switch to remain: {result}"
    );
}
