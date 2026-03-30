//! Folds constant expressions at compile time.
//!
//! Handles:
//! - Numeric binary operations: `1 + 2` -> `3`
//! - Boolean negation: `!true` -> `false`
//! - Logical not on literals: `!0` -> `true`, `!""` -> `true`, `![]` -> `false`
//! - Unary plus coercion: `+true` -> `1`, `+[]` -> `0`, `+"5"` -> `5`
//! - Unary negation coercion: `-true` -> `-1`
//! - String concatenation of literals (basic cases): `"a" + "b"` -> `"ab"`
//! - Typeof on literals: `typeof "hello"` -> `"string"`
//!
//! These coercion rules enable multi-pass simplification of JSFuck-style patterns:
//! `!![]` -> `!false` -> `true`, `+!![]` -> `+true` -> `1`

use oxc_ast::ast::Expression;
use oxc_span::SPAN;
use oxc_syntax::number::NumberBase;
use oxc_syntax::operator::{BinaryOperator, LogicalOperator, UnaryOperator};
use oxc_traverse::TraverseCtx;

use crate::operations;
use crate::transformer::{AstNodeType, Transformer, TransformerPhase, TransformerPriority};

/// Folds constant binary and unary expressions into their computed values.
pub struct ConstantFoldingTransformer;

impl Transformer for ConstantFoldingTransformer {
    fn name(&self) -> &str {
        "ConstantFoldingTransformer"
    }

    fn interests(&self) -> &[AstNodeType] {
        &[
            AstNodeType::BinaryExpression,
            AstNodeType::UnaryExpression,
            AstNodeType::LogicalExpression,
        ]
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
        match expression {
            Expression::BinaryExpression(_) => try_fold_binary_expression(expression, context),
            Expression::UnaryExpression(_) => try_fold_unary_expression(expression, context),
            Expression::LogicalExpression(_) => {
                try_fold_logical_expression(expression, context)
            }
            _ => false,
        }
    }
}

/// Helper: create a numeric literal expression.
fn make_numeric_literal<'a>(context: &TraverseCtx<'a, ()>, value: f64) -> Expression<'a> {
    let raw = context.ast.atom(&value.to_string());
    context
        .ast
        .expression_numeric_literal(SPAN, value, Some(raw), NumberBase::Decimal)
}

/// Helper: create a string literal expression.
fn make_string_literal<'a>(context: &TraverseCtx<'a, ()>, value: &str) -> Expression<'a> {
    let atom = context.ast.atom(value);
    context.ast.expression_string_literal(SPAN, atom, None)
}

/// Unwrap parenthesized expressions to get the inner expression.
fn unwrap_parens<'a, 'b>(expression: &'b Expression<'a>) -> &'b Expression<'a> {
    let mut current = expression;
    while let Expression::ParenthesizedExpression(paren) = current {
        current = &paren.expression;
    }
    current
}

/// Try to fold a binary expression with two literal operands.
fn try_fold_binary_expression<'a>(
    expression: &mut Expression<'a>,
    context: &mut TraverseCtx<'a, ()>,
) -> bool {
    let Expression::BinaryExpression(binary) = expression else {
        return false;
    };

    // Try numeric folding: both sides are numeric literals (looking through parens).
    if let (Expression::NumericLiteral(left), Expression::NumericLiteral(right)) =
        (unwrap_parens(&binary.left), unwrap_parens(&binary.right))
    {
        let left_value = left.value;
        let right_value = right.value;

        let result = match binary.operator {
            BinaryOperator::Addition => Some(left_value + right_value),
            BinaryOperator::Subtraction => Some(left_value - right_value),
            BinaryOperator::Multiplication => Some(left_value * right_value),
            BinaryOperator::Division if right_value != 0.0 => Some(left_value / right_value),
            BinaryOperator::Remainder if right_value != 0.0 => Some(left_value % right_value),
            BinaryOperator::Exponential => Some(left_value.powf(right_value)),
            _ => None,
        };

        if let Some(result_value) = result {
            let replacement = make_numeric_literal(context, result_value);
            operations::replace_expression(expression, replacement, context);
            return true;
        }

        // Try numeric comparison.
        let comparison_result = match binary.operator {
            BinaryOperator::LessThan => Some(left_value < right_value),
            BinaryOperator::LessEqualThan => Some(left_value <= right_value),
            BinaryOperator::GreaterThan => Some(left_value > right_value),
            BinaryOperator::GreaterEqualThan => Some(left_value >= right_value),
            BinaryOperator::StrictEquality => Some(left_value == right_value),
            BinaryOperator::StrictInequality => Some(left_value != right_value),
            BinaryOperator::Equality => Some(left_value == right_value),
            BinaryOperator::Inequality => Some(left_value != right_value),
            _ => None,
        };

        if let Some(result_value) = comparison_result {
            let replacement = context.ast.expression_boolean_literal(SPAN, result_value);
            operations::replace_expression(expression, replacement, context);
            return true;
        }

        // Try bitwise operations (convert to i32 as JavaScript does).
        let left_int = left_value as i32;
        let right_int = right_value as i32;

        let bitwise_result = match binary.operator {
            BinaryOperator::BitwiseOR => Some(left_int | right_int),
            BinaryOperator::BitwiseAnd => Some(left_int & right_int),
            BinaryOperator::BitwiseXOR => Some(left_int ^ right_int),
            BinaryOperator::ShiftLeft => Some(left_int << (right_int & 0x1f)),
            BinaryOperator::ShiftRight => Some(left_int >> (right_int & 0x1f)),
            BinaryOperator::ShiftRightZeroFill => {
                // Unsigned right shift: treat as u32 to match JavaScript semantics.
                let result = (left_int as u32) >> (right_int as u32 & 0x1f);
                Some(result as i32)
            }
            _ => None,
        };

        if let Some(result_value) = bitwise_result {
            let replacement = make_numeric_literal(context, f64::from(result_value));
            operations::replace_expression(expression, replacement, context);
            return true;
        }
    }

    // Try string concatenation: both sides are string literals (looking through parens).
    if binary.operator == BinaryOperator::Addition
        && let (Expression::StringLiteral(left), Expression::StringLiteral(right)) =
            (unwrap_parens(&binary.left), unwrap_parens(&binary.right))
        {
            let mut concatenated = left.value.to_string();
            concatenated.push_str(&right.value);
            let replacement = make_string_literal(context, &concatenated);
            operations::replace_expression(expression, replacement, context);
            return true;
        }

    false
}

/// Try to fold a unary expression with a literal operand.
fn try_fold_unary_expression<'a>(
    expression: &mut Expression<'a>,
    context: &mut TraverseCtx<'a, ()>,
) -> bool {
    let Expression::UnaryExpression(unary) = expression else {
        return false;
    };

    match unary.operator {
        UnaryOperator::LogicalNot => {
            let argument = unwrap_parens(&unary.argument);
            let result = match argument {
                Expression::BooleanLiteral(boolean) => Some(!boolean.value),
                // !0 -> true, !NaN -> true, !<nonzero> -> false
                Expression::NumericLiteral(number) => {
                    Some(number.value == 0.0 || number.value.is_nan())
                }
                // !"" -> true, !"hello" -> false
                Expression::StringLiteral(string) => Some(string.value.is_empty()),
                // !null -> true
                Expression::NullLiteral(_) => Some(true),
                // ![] -> false (arrays are always truthy in JavaScript)
                Expression::ArrayExpression(_) => Some(false),
                // !{} -> false (objects are always truthy)
                Expression::ObjectExpression(_) => Some(false),
                _ => None,
            };
            if let Some(result_value) = result {
                let replacement = context.ast.expression_boolean_literal(SPAN, result_value);
                operations::replace_expression(expression, replacement, context);
                return true;
            }
        }
        UnaryOperator::UnaryNegation => {
            let argument = unwrap_parens(&unary.argument);
            let result = match argument {
                Expression::NumericLiteral(number) => Some(-number.value),
                // -true -> -1, -false -> -0 (we emit -0 as 0)
                Expression::BooleanLiteral(boolean) => {
                    Some(if boolean.value { -1.0 } else { -0.0 })
                }
                // -null -> -0
                Expression::NullLiteral(_) => Some(-0.0),
                // -[] -> -0 (empty array coerces to 0)
                Expression::ArrayExpression(array) if array.elements.is_empty() => Some(-0.0),
                _ => None,
            };
            if let Some(result_value) = result {
                let replacement = make_numeric_literal(context, result_value);
                operations::replace_expression(expression, replacement, context);
                return true;
            }
        }
        UnaryOperator::UnaryPlus => {
            let argument = unwrap_parens(&unary.argument);
            let result = match argument {
                Expression::NumericLiteral(number) => Some(number.value),
                // +true -> 1, +false -> 0
                Expression::BooleanLiteral(boolean) => {
                    Some(if boolean.value { 1.0 } else { 0.0 })
                }
                // +null -> 0
                Expression::NullLiteral(_) => Some(0.0),
                // +[] -> 0 (empty array coerces to "")
                Expression::ArrayExpression(array) if array.elements.is_empty() => Some(0.0),
                // +"5" -> 5, +"" -> 0, +"3.14" -> 3.14
                Expression::StringLiteral(string) => {
                    let trimmed = string.value.as_str().trim();
                    if trimmed.is_empty() {
                        Some(0.0)
                    } else {
                        trimmed.parse::<f64>().ok()
                    }
                }
                _ => None,
            };
            if let Some(result_value) = result {
                let replacement = make_numeric_literal(context, result_value);
                operations::replace_expression(expression, replacement, context);
                return true;
            }
        }
        UnaryOperator::Typeof => {
            let type_name = match &unary.argument {
                Expression::StringLiteral(_) => Some("string"),
                Expression::NumericLiteral(_) => Some("number"),
                Expression::BooleanLiteral(_) => Some("boolean"),
                Expression::NullLiteral(_) => Some("object"),
                Expression::FunctionExpression(_) | Expression::ArrowFunctionExpression(_) => {
                    Some("function")
                }
                _ => None,
            };

            if let Some(type_name) = type_name {
                let replacement = make_string_literal(context, type_name);
                operations::replace_expression(expression, replacement, context);
                return true;
            }
        }
        UnaryOperator::Void => {
            // `void <expr>` always evaluates to `undefined` when the argument is
            // side-effect-free. For safety we only fold `void <literal>`.
            let argument = unwrap_parens(&unary.argument);
            if matches!(
                argument,
                Expression::NumericLiteral(_)
                    | Expression::StringLiteral(_)
                    | Expression::BooleanLiteral(_)
                    | Expression::NullLiteral(_)
            ) {
                let atom = context.ast.atom("undefined");
                let replacement = context.ast.expression_identifier(SPAN, atom);
                operations::replace_expression(expression, replacement, context);
                return true;
            }
        }
        UnaryOperator::BitwiseNot => {
            if let Expression::NumericLiteral(number) = &unary.argument {
                let result = !(number.value as i32);
                let replacement = make_numeric_literal(context, f64::from(result));
                operations::replace_expression(expression, replacement, context);
                return true;
            }
        }
        _ => {}
    }

    false
}

/// Determine the static truthiness of an expression.
/// Returns `Some(true)` if definitely truthy, `Some(false)` if definitely falsy,
/// `None` if unknown.
fn static_truthiness(expression: &Expression<'_>) -> Option<bool> {
    let expression = unwrap_parens(expression);
    match expression {
        Expression::BooleanLiteral(literal) => Some(literal.value),
        Expression::NumericLiteral(literal) => Some(literal.value != 0.0 && !literal.value.is_nan()),
        Expression::StringLiteral(literal) => Some(!literal.value.is_empty()),
        Expression::NullLiteral(_) => Some(false),
        Expression::ArrayExpression(_) | Expression::ObjectExpression(_) => Some(true),
        _ => None,
    }
}

/// Simplify logical expressions (`&&`, `||`) when one operand has a known truthiness.
///
/// - `x && false` → `(x, false)` — x may have side effects
/// - `false && x` → `false` — short-circuit, x is never evaluated
/// - `x || true` → `(x, true)` — x may have side effects
/// - `true || x` → `true` — short-circuit, x is never evaluated
/// - `x && true` → `x` — the `&& true` is redundant
/// - `true && x` → `x` — always evaluates to x
/// - `x || false` → `x` — the `|| false` is redundant
/// - `false || x` → `x` — always evaluates to x
fn try_fold_logical_expression<'a>(
    expression: &mut Expression<'a>,
    context: &mut TraverseCtx<'a, ()>,
) -> bool {
    let Expression::LogicalExpression(logical) = expression else {
        return false;
    };

    // Only handle && and ||, not ??
    let operator = logical.operator;
    if operator == LogicalOperator::Coalesce {
        return false;
    }

    let left_truthiness = static_truthiness(&logical.left);
    let right_truthiness = static_truthiness(&logical.right);

    match operator {
        LogicalOperator::And => {
            if let Some(false) = right_truthiness {
                // `x && false` → `(x, false)` — preserve x for side effects
                let left = std::mem::replace(
                    &mut logical.left,
                    context.ast.expression_null_literal(SPAN),
                );
                let right = std::mem::replace(
                    &mut logical.right,
                    context.ast.expression_null_literal(SPAN),
                );
                let mut expressions = context.ast.vec_with_capacity(2);
                expressions.push(left);
                expressions.push(right);
                operations::replace_expression_with_sequence(expression, expressions, context);
                return true;
            }
            if let Some(false) = left_truthiness {
                // `false && x` → `false` — short-circuit, x is never evaluated
                let left = std::mem::replace(
                    &mut logical.left,
                    context.ast.expression_null_literal(SPAN),
                );
                operations::replace_expression(expression, left, context);
                return true;
            }
            if let Some(true) = right_truthiness {
                // `x && true` → `x`
                let left = std::mem::replace(
                    &mut logical.left,
                    context.ast.expression_null_literal(SPAN),
                );
                operations::replace_expression(expression, left, context);
                return true;
            }
            if let Some(true) = left_truthiness {
                // `true && x` → `x`
                let right = std::mem::replace(
                    &mut logical.right,
                    context.ast.expression_null_literal(SPAN),
                );
                operations::replace_expression(expression, right, context);
                return true;
            }
        }
        LogicalOperator::Or => {
            if let Some(true) = right_truthiness {
                // `x || true` → `(x, true)` — preserve x for side effects
                let left = std::mem::replace(
                    &mut logical.left,
                    context.ast.expression_null_literal(SPAN),
                );
                let right = std::mem::replace(
                    &mut logical.right,
                    context.ast.expression_null_literal(SPAN),
                );
                let mut expressions = context.ast.vec_with_capacity(2);
                expressions.push(left);
                expressions.push(right);
                operations::replace_expression_with_sequence(expression, expressions, context);
                return true;
            }
            if let Some(true) = left_truthiness {
                // `true || x` → `true` — short-circuit, x is never evaluated
                let left = std::mem::replace(
                    &mut logical.left,
                    context.ast.expression_null_literal(SPAN),
                );
                operations::replace_expression(expression, left, context);
                return true;
            }
            if let Some(false) = right_truthiness {
                // `x || false` → `x`
                let left = std::mem::replace(
                    &mut logical.left,
                    context.ast.expression_null_literal(SPAN),
                );
                operations::replace_expression(expression, left, context);
                return true;
            }
            if let Some(false) = left_truthiness {
                // `false || x` → `x`
                let right = std::mem::replace(
                    &mut logical.right,
                    context.ast.expression_null_literal(SPAN),
                );
                operations::replace_expression(expression, right, context);
                return true;
            }
        }
        _ => {}
    }

    false
}
