//! Integration tests for the BitwiseSimplificationTransformer.

mod common;
use common::deobfuscate;

// ---------------------------------------------------------------------------
// XOR equivalences
// ---------------------------------------------------------------------------

#[test]
fn xor_via_and_or_not() {
    // (A & ~B) | (~A & B) = A ^ B
    assert_eq!(deobfuscate("var x = (a & ~b) | (~a & b);"), "var x = a ^ b;");
}

#[test]
fn xor_via_or_and_nand() {
    // (A | B) & ~(A & B) = A ^ B
    assert_eq!(
        deobfuscate("var x = (a | b) & ~(a & b);"),
        "var x = a ^ b;"
    );
}

// ---------------------------------------------------------------------------
// De Morgan's law
// ---------------------------------------------------------------------------

#[test]
fn de_morgan_and() {
    // ~(~A | ~B) = A & B
    assert_eq!(deobfuscate("var x = ~(~a | ~b);"), "var x = a & b;");
}

#[test]
fn de_morgan_or() {
    // ~(~A & ~B) = A | B
    assert_eq!(deobfuscate("var x = ~(~a & ~b);"), "var x = a | b;");
}

// ---------------------------------------------------------------------------
// Two's complement negation
// ---------------------------------------------------------------------------

#[test]
fn twos_complement_negation() {
    // ~A + 1 = -A
    assert_eq!(deobfuscate("var x = ~a + 1;"), "var x = -a;");
}

#[test]
fn twos_complement_negation_alt() {
    // (A ^ -1) + 1 = -A  (since A ^ -1 = ~A)
    assert_eq!(deobfuscate("var x = (a ^ -1) + 1;"), "var x = -a;");
}

// ---------------------------------------------------------------------------
// Double NOT identity
// ---------------------------------------------------------------------------

#[test]
fn double_not_identity() {
    assert_eq!(deobfuscate("var x = ~~a;"), "var x = a;");
}

// ---------------------------------------------------------------------------
// Identity patterns (no-ops)
// ---------------------------------------------------------------------------

#[test]
fn xor_zero_identity() {
    assert_eq!(deobfuscate("var x = a ^ 0;"), "var x = a;");
}

#[test]
fn or_zero_identity() {
    assert_eq!(deobfuscate("var x = a | 0;"), "var x = a;");
}

#[test]
fn and_all_ones_identity() {
    assert_eq!(deobfuscate("var x = a & -1;"), "var x = a;");
}

#[test]
fn add_zero_identity() {
    assert_eq!(deobfuscate("var x = a + 0;"), "var x = a;");
}

// ---------------------------------------------------------------------------
// Constant results
// ---------------------------------------------------------------------------

#[test]
fn xor_self_is_zero() {
    assert_eq!(deobfuscate("var x = a ^ a;"), "var x = 0;");
}

#[test]
fn or_complement_is_all_ones() {
    assert_eq!(deobfuscate("var x = a | ~a;"), "var x = -1;");
}

#[test]
fn and_complement_is_zero() {
    assert_eq!(deobfuscate("var x = a & ~a;"), "var x = 0;");
}

// ---------------------------------------------------------------------------
// Addition via bitwise decomposition
// ---------------------------------------------------------------------------

#[test]
fn addition_via_carry_decomposition() {
    // (A ^ B) + 2 * (A & B) = A + B
    assert_eq!(
        deobfuscate("var x = (a ^ b) + 2 * (a & b);"),
        "var x = a + b;"
    );
}

#[test]
fn addition_via_or_and() {
    // (A | B) + (A & B) = A + B
    assert_eq!(
        deobfuscate("var x = (a | b) + (a & b);"),
        "var x = a + b;"
    );
}

// ---------------------------------------------------------------------------
// Subtraction via bitwise
// ---------------------------------------------------------------------------

#[test]
fn subtraction_via_complement() {
    // A + ~B + 1 = A - B
    assert_eq!(deobfuscate("var x = a + ~b + 1;"), "var x = a - b;");
}

// ---------------------------------------------------------------------------
// Negation via complement
// ---------------------------------------------------------------------------

#[test]
fn negation_via_not_plus_one() {
    // -(~A + 1) is just double negation -> A? No: -(~A + 1) = -(-A) = A
    // Wait, ~A + 1 = -A, so -(~A + 1) = -(-A) = A
    // This would be caught as IdentityA after two passes.
    // First pass: ~a + 1 -> -a. Second pass: -(-a) -> a? That needs constant folding
    // on unary negation of unary negation, which isn't in scope.
    // Let's just test the single-step case.
    assert_eq!(deobfuscate("var x = ~a + 1;"), "var x = -a;");
}

// ---------------------------------------------------------------------------
// No-op: already simple expressions should pass through
// ---------------------------------------------------------------------------

#[test]
fn simple_xor_unchanged() {
    assert_eq!(deobfuscate("var x = a ^ b;"), "var x = a ^ b;");
}

#[test]
fn simple_and_unchanged() {
    assert_eq!(deobfuscate("var x = a & b;"), "var x = a & b;");
}

#[test]
fn simple_not_unchanged() {
    assert_eq!(deobfuscate("var x = ~a;"), "var x = ~a;");
}
