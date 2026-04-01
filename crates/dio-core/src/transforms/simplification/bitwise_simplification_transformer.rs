//! Simplifies complex bitwise and mixed boolean-arithmetic (MBA) expressions
//! using truth table evaluation.
//!
//! Obfuscators often rewrite simple operations as complex equivalents:
//! - `(A & ~B) | (~A & B)` -> `A ^ B`
//! - `~(~A | ~B)` -> `A & B` (De Morgan's law)
//! - `~(~A & ~B)` -> `A | B` (De Morgan's law)
//! - `~A + 1` -> `-A` (two's complement negation)
//! - `(A ^ B) + 2 * (A & B)` -> `A + B` (carry decomposition)
//! - `(A | B) + (A & B)` -> `A + B`
//! - `~~A` -> `A` (double bitwise NOT)
//! - `A ^ 0` -> `A`, `A | 0` -> `A`, `A & -1` -> `A`
//!
//! Rather than pattern-matching each rewrite rule, this transformer evaluates
//! the expression at multiple test points and matches the results against a
//! table of canonical operations. This handles arbitrary nesting and composition
//! automatically.

use oxc_ast::ast::Expression;
use oxc_span::SPAN;
use oxc_syntax::number::NumberBase;
use oxc_syntax::operator::{BinaryOperator, UnaryOperator};
use oxc_traverse::TraverseCtx;

use crate::operations;
use crate::transformer::{AstNodeType, Transformer, TransformerPhase, TransformerPriority};
use crate::utils::{unwrap_parens, unwrap_parens_mut};

/// Simplifies complex bitwise and mixed boolean-arithmetic expressions by
/// identifying the canonical operation through truth table evaluation.
pub struct BitwiseSimplificationTransformer;

impl Transformer for BitwiseSimplificationTransformer {
    fn name(&self) -> &str {
        "BitwiseSimplificationTransformer"
    }

    fn interests(&self) -> &[AstNodeType] {
        &[AstNodeType::BinaryExpression, AstNodeType::UnaryExpression]
    }

    fn priority(&self) -> TransformerPriority {
        TransformerPriority::Default
    }

    fn phase(&self) -> TransformerPhase {
        TransformerPhase::Main
    }

    fn enter_expression<'a>(
        &self,
        expression: &mut Expression<'a>,
        context: &mut TraverseCtx<'a, ()>,
    ) -> bool {
        try_simplify_bitwise(expression, context)
    }
}

// ---------------------------------------------------------------------------
// Main simplification logic
// ---------------------------------------------------------------------------

/// Test value pairs for two-operand evaluation. Chosen to distinguish all 16
/// binary boolean functions and common arithmetic operations.
const TEST_PAIRS: [(i32, i32); 8] = [
    (0, 0),
    (0, -1),
    (-1, 0),
    (-1, -1),
    (0x5555_5555, 0x3333_3333),
    (1, 2),
    (-42, 17),
    (0x7FFF_FFFF, 1),
];

/// Test values for single-operand evaluation.
const TEST_SINGLES: [i32; 8] = [0, 1, -1, 0x5555_5555, 42, -42, 0x7FFF_FFFF, 100];

/// Try to simplify a bitwise/arithmetic expression by matching it against
/// canonical operations via truth table evaluation.
fn try_simplify_bitwise<'a>(
    expression: &mut Expression<'a>,
    context: &mut TraverseCtx<'a, ()>,
) -> bool {
    // Expressions with fewer than 3 nodes are already minimal.
    let original_cost = expression_node_count(&*expression);
    if original_cost < 3 {
        return false;
    }

    // Collect unique identifier operands. Bail if the tree contains unsupported
    // operations (function calls, property access, etc.) or more than 2 operands.
    let mut operand_names: Vec<String> = Vec::new();
    if !collect_operands(&*expression, &mut operand_names) {
        return false;
    }
    if operand_names.is_empty() {
        // Pure constant expression — the constant folding transformer handles these.
        return false;
    }

    // Evaluate at multiple test points and match against canonical operations.
    let canonical = match operand_names.len() {
        1 => try_match_single_operand(&*expression, &operand_names[0]),
        2 => try_match_two_operands(&*expression, &operand_names[0], &operand_names[1]),
        _ => return false,
    };

    let Some(canonical) = canonical else {
        return false;
    };

    // Only simplify if the canonical form has strictly fewer nodes.
    if canonical_node_count(&canonical) >= original_cost {
        return false;
    }

    // Extract one occurrence of each needed operand from the original tree,
    // preserving their scoping bindings. Remaining occurrences will be cleaned
    // up by operations::replace_expression.
    let operand_a = if canonical_needs_operand_a(&canonical) {
        extract_first_identifier(expression, &operand_names[0], context)
    } else {
        None
    };
    let operand_b = if canonical_needs_operand_b(&canonical) && operand_names.len() > 1 {
        extract_first_identifier(expression, &operand_names[1], context)
    } else {
        None
    };

    let replacement = build_canonical_expression(canonical, operand_a, operand_b, context);
    operations::replace_expression(expression, replacement, context);
    true
}

// ---------------------------------------------------------------------------
// Canonical operation representation
// ---------------------------------------------------------------------------

/// The set of simple operations an MBA expression might be equivalent to.
#[derive(Clone, Copy, Debug)]
enum CanonicalOperation {
    Constant(i32),
    IdentityA,
    IdentityB,
    BitwiseNotA,
    BitwiseNotB,
    NegateA,
    NegateB,
    And,
    Or,
    Xor,
    Add,
    SubtractAB,
    SubtractBA,
}

/// Evaluate a canonical operation given two operand values.
fn evaluate_canonical(operation: CanonicalOperation, a: i32, b: i32) -> i32 {
    match operation {
        CanonicalOperation::Constant(c) => c,
        CanonicalOperation::IdentityA => a,
        CanonicalOperation::IdentityB => b,
        CanonicalOperation::BitwiseNotA => !a,
        CanonicalOperation::BitwiseNotB => !b,
        CanonicalOperation::NegateA => a.wrapping_neg(),
        CanonicalOperation::NegateB => b.wrapping_neg(),
        CanonicalOperation::And => a & b,
        CanonicalOperation::Or => a | b,
        CanonicalOperation::Xor => a ^ b,
        CanonicalOperation::Add => a.wrapping_add(b),
        CanonicalOperation::SubtractAB => a.wrapping_sub(b),
        CanonicalOperation::SubtractBA => b.wrapping_sub(a),
    }
}

/// Number of AST nodes the canonical operation would produce.
fn canonical_node_count(operation: &CanonicalOperation) -> usize {
    match operation {
        CanonicalOperation::Constant(_)
        | CanonicalOperation::IdentityA
        | CanonicalOperation::IdentityB => 1,
        CanonicalOperation::BitwiseNotA
        | CanonicalOperation::BitwiseNotB
        | CanonicalOperation::NegateA
        | CanonicalOperation::NegateB => 2,
        CanonicalOperation::And
        | CanonicalOperation::Or
        | CanonicalOperation::Xor
        | CanonicalOperation::Add
        | CanonicalOperation::SubtractAB
        | CanonicalOperation::SubtractBA => 3,
    }
}

/// Whether the canonical operation requires operand A.
fn canonical_needs_operand_a(operation: &CanonicalOperation) -> bool {
    !matches!(
        operation,
        CanonicalOperation::Constant(_)
            | CanonicalOperation::IdentityB
            | CanonicalOperation::BitwiseNotB
            | CanonicalOperation::NegateB
    )
}

/// Whether the canonical operation requires operand B.
fn canonical_needs_operand_b(operation: &CanonicalOperation) -> bool {
    !matches!(
        operation,
        CanonicalOperation::Constant(_)
            | CanonicalOperation::IdentityA
            | CanonicalOperation::BitwiseNotA
            | CanonicalOperation::NegateA
    )
}

// ---------------------------------------------------------------------------
// Canonical matching via test-point evaluation
// ---------------------------------------------------------------------------

/// Candidates for single-operand expressions.
const SINGLE_OPERAND_CANDIDATES: &[CanonicalOperation] = &[
    CanonicalOperation::IdentityA,
    CanonicalOperation::BitwiseNotA,
    CanonicalOperation::NegateA,
];

/// Candidates for two-operand expressions (constants handled separately).
const TWO_OPERAND_CANDIDATES: &[CanonicalOperation] = &[
    CanonicalOperation::IdentityA,
    CanonicalOperation::IdentityB,
    CanonicalOperation::BitwiseNotA,
    CanonicalOperation::BitwiseNotB,
    CanonicalOperation::NegateA,
    CanonicalOperation::NegateB,
    CanonicalOperation::And,
    CanonicalOperation::Or,
    CanonicalOperation::Xor,
    CanonicalOperation::Add,
    CanonicalOperation::SubtractAB,
    CanonicalOperation::SubtractBA,
];

/// Try to match a single-operand expression against canonical operations.
fn try_match_single_operand(
    expression: &Expression<'_>,
    operand_name: &str,
) -> Option<CanonicalOperation> {
    let results: Vec<i32> = TEST_SINGLES
        .iter()
        .map(|&a| evaluate_with_values(expression, &[(operand_name, a)]))
        .collect::<Option<Vec<_>>>()?;

    // Check for constant result first.
    if results.iter().all(|&r| r == results[0]) {
        return Some(CanonicalOperation::Constant(results[0]));
    }

    for &candidate in SINGLE_OPERAND_CANDIDATES {
        let matches = TEST_SINGLES
            .iter()
            .zip(&results)
            .all(|(&a, &result)| evaluate_canonical(candidate, a, 0) == result);
        if matches {
            return Some(candidate);
        }
    }

    None
}

/// Try to match a two-operand expression against canonical operations.
fn try_match_two_operands(
    expression: &Expression<'_>,
    operand_a: &str,
    operand_b: &str,
) -> Option<CanonicalOperation> {
    let results: Vec<i32> = TEST_PAIRS
        .iter()
        .map(|&(a, b)| evaluate_with_values(expression, &[(operand_a, a), (operand_b, b)]))
        .collect::<Option<Vec<_>>>()?;

    // Check for constant result first.
    if results.iter().all(|&r| r == results[0]) {
        return Some(CanonicalOperation::Constant(results[0]));
    }

    for &candidate in TWO_OPERAND_CANDIDATES {
        let matches = TEST_PAIRS
            .iter()
            .zip(&results)
            .all(|(&(a, b), &result)| evaluate_canonical(candidate, a, b) == result);
        if matches {
            return Some(candidate);
        }
    }

    None
}

// ---------------------------------------------------------------------------
// Expression evaluation with substituted values
// ---------------------------------------------------------------------------

/// Evaluate an expression tree with identifier values substituted from the map.
/// Returns None if the expression contains unsupported operations.
fn evaluate_with_values(expression: &Expression<'_>, values: &[(&str, i32)]) -> Option<i32> {
    match unwrap_parens(expression) {
        Expression::NumericLiteral(number) => Some(number.value as i32),
        Expression::Identifier(identifier) => {
            let name = identifier.name.as_str();
            values.iter().find(|(n, _)| *n == name).map(|(_, v)| *v)
        }
        Expression::UnaryExpression(unary) => {
            let argument = evaluate_with_values(&unary.argument, values)?;
            match unary.operator {
                UnaryOperator::BitwiseNot => Some(!argument),
                UnaryOperator::UnaryNegation => Some(argument.wrapping_neg()),
                UnaryOperator::UnaryPlus => Some(argument),
                _ => None,
            }
        }
        Expression::BinaryExpression(binary) => {
            let left = evaluate_with_values(&binary.left, values)?;
            let right = evaluate_with_values(&binary.right, values)?;
            match binary.operator {
                BinaryOperator::BitwiseAnd => Some(left & right),
                BinaryOperator::BitwiseOR => Some(left | right),
                BinaryOperator::BitwiseXOR => Some(left ^ right),
                BinaryOperator::Addition => Some(left.wrapping_add(right)),
                BinaryOperator::Subtraction => Some(left.wrapping_sub(right)),
                BinaryOperator::Multiplication => Some(left.wrapping_mul(right)),
                BinaryOperator::ShiftLeft => Some(left.wrapping_shl((right & 0x1f) as u32)),
                BinaryOperator::ShiftRight => Some(left.wrapping_shr((right & 0x1f) as u32)),
                BinaryOperator::ShiftRightZeroFill => {
                    Some((left as u32).wrapping_shr((right & 0x1f) as u32) as i32)
                }
                _ => None,
            }
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Operand collection
// ---------------------------------------------------------------------------

/// Walk the expression tree and collect unique identifier operand names.
/// Returns false if the tree contains unsupported nodes or more than 2 operands.
fn collect_operands(expression: &Expression<'_>, names: &mut Vec<String>) -> bool {
    match unwrap_parens(expression) {
        Expression::NumericLiteral(_) => true,
        Expression::Identifier(identifier) => {
            let name = identifier.name.as_str();
            if !names.iter().any(|n| n == name) {
                if names.len() >= 2 {
                    return false;
                }
                names.push(name.to_owned());
            }
            true
        }
        Expression::UnaryExpression(unary) => {
            matches!(
                unary.operator,
                UnaryOperator::BitwiseNot | UnaryOperator::UnaryNegation | UnaryOperator::UnaryPlus
            ) && collect_operands(&unary.argument, names)
        }
        Expression::BinaryExpression(binary) => {
            matches!(
                binary.operator,
                BinaryOperator::BitwiseAnd
                    | BinaryOperator::BitwiseOR
                    | BinaryOperator::BitwiseXOR
                    | BinaryOperator::Addition
                    | BinaryOperator::Subtraction
                    | BinaryOperator::Multiplication
                    | BinaryOperator::ShiftLeft
                    | BinaryOperator::ShiftRight
                    | BinaryOperator::ShiftRightZeroFill
            ) && collect_operands(&binary.left, names)
                && collect_operands(&binary.right, names)
        }
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Cost helpers
// ---------------------------------------------------------------------------

/// Count the number of AST nodes in an expression tree.
fn expression_node_count(expression: &Expression<'_>) -> usize {
    match unwrap_parens(expression) {
        Expression::UnaryExpression(unary) => 1 + expression_node_count(&unary.argument),
        Expression::BinaryExpression(binary) => {
            1 + expression_node_count(&binary.left) + expression_node_count(&binary.right)
        }
        _ => 1,
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Create a numeric literal expression.
fn make_numeric_literal<'a>(context: &TraverseCtx<'a, ()>, value: f64) -> Expression<'a> {
    let raw = context.ast.atom(&format_number(value));
    context
        .ast
        .expression_numeric_literal(SPAN, value, Some(raw), NumberBase::Decimal)
}

/// Format a number for the raw literal string, omitting `.0` for integers.
fn format_number(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < (i64::MAX as f64) {
        format!("{}", value as i64)
    } else {
        value.to_string()
    }
}

// ---------------------------------------------------------------------------
// AST construction for canonical operations
// ---------------------------------------------------------------------------

/// Build an AST expression from a canonical operation and extracted operands.
fn build_canonical_expression<'a>(
    operation: CanonicalOperation,
    operand_a: Option<Expression<'a>>,
    operand_b: Option<Expression<'a>>,
    context: &TraverseCtx<'a, ()>,
) -> Expression<'a> {
    match operation {
        CanonicalOperation::Constant(value) => make_numeric_literal(context, f64::from(value)),
        CanonicalOperation::IdentityA => operand_a.unwrap(),
        CanonicalOperation::IdentityB => operand_b.unwrap(),
        CanonicalOperation::BitwiseNotA => {
            context
                .ast
                .expression_unary(SPAN, UnaryOperator::BitwiseNot, operand_a.unwrap())
        }
        CanonicalOperation::BitwiseNotB => {
            context
                .ast
                .expression_unary(SPAN, UnaryOperator::BitwiseNot, operand_b.unwrap())
        }
        CanonicalOperation::NegateA => {
            context
                .ast
                .expression_unary(SPAN, UnaryOperator::UnaryNegation, operand_a.unwrap())
        }
        CanonicalOperation::NegateB => {
            context
                .ast
                .expression_unary(SPAN, UnaryOperator::UnaryNegation, operand_b.unwrap())
        }
        CanonicalOperation::And => context.ast.expression_binary(
            SPAN,
            operand_a.unwrap(),
            BinaryOperator::BitwiseAnd,
            operand_b.unwrap(),
        ),
        CanonicalOperation::Or => context.ast.expression_binary(
            SPAN,
            operand_a.unwrap(),
            BinaryOperator::BitwiseOR,
            operand_b.unwrap(),
        ),
        CanonicalOperation::Xor => context.ast.expression_binary(
            SPAN,
            operand_a.unwrap(),
            BinaryOperator::BitwiseXOR,
            operand_b.unwrap(),
        ),
        CanonicalOperation::Add => context.ast.expression_binary(
            SPAN,
            operand_a.unwrap(),
            BinaryOperator::Addition,
            operand_b.unwrap(),
        ),
        CanonicalOperation::SubtractAB => context.ast.expression_binary(
            SPAN,
            operand_a.unwrap(),
            BinaryOperator::Subtraction,
            operand_b.unwrap(),
        ),
        CanonicalOperation::SubtractBA => context.ast.expression_binary(
            SPAN,
            operand_b.unwrap(),
            BinaryOperator::Subtraction,
            operand_a.unwrap(),
        ),
    }
}

// ---------------------------------------------------------------------------
// Operand extraction
// ---------------------------------------------------------------------------

/// Extract the first occurrence of an identifier with the given name from the
/// expression tree, replacing it with a dummy literal. The extracted expression
/// retains its scoping bindings; remaining occurrences are cleaned up by
/// `operations::replace_expression`.
fn extract_first_identifier<'a>(
    expression: &mut Expression<'a>,
    name: &str,
    context: &TraverseCtx<'a, ()>,
) -> Option<Expression<'a>> {
    // Check without holding a borrow into expression, to allow mem::replace.
    let is_target = matches!(
        unwrap_parens(&*expression),
        Expression::Identifier(identifier) if identifier.name.as_str() == name
    );
    if is_target {
        let dummy = make_numeric_literal(context, 0.0);
        return Some(std::mem::replace(unwrap_parens_mut(expression), dummy));
    }

    match expression {
        Expression::ParenthesizedExpression(paren) => {
            extract_first_identifier(&mut paren.expression, name, context)
        }
        Expression::UnaryExpression(unary) => {
            extract_first_identifier(&mut unary.argument, name, context)
        }
        Expression::BinaryExpression(binary) => {
            extract_first_identifier(&mut binary.left, name, context)
                .or_else(|| extract_first_identifier(&mut binary.right, name, context))
        }
        _ => None,
    }
}
