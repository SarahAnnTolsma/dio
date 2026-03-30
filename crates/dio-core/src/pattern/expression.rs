//! Expression pattern matching.

use std::sync::Arc;

use oxc_ast::ast::Expression;
use oxc_syntax::operator::{BinaryOperator, UnaryOperator};

use super::capture::{CapturedNode, MatchResult};

/// A composable pattern that matches against JavaScript expression AST nodes.
///
/// Patterns are data structures — they can be cloned, inspected, and composed.
/// Use the combinator functions in `combinators` for ergonomic construction.
#[derive(Clone)]
pub enum ExpressionPattern {
    // -- Meta-patterns --
    /// Matches any expression.
    Any,

    /// Matches any literal (number, string, boolean, null).
    AnyLiteral,

    /// Matches the inner pattern and captures the matched node's value under the given name.
    Capture(String, Box<ExpressionPattern>),

    /// All sub-patterns must match the same expression.
    And(Vec<ExpressionPattern>),

    /// At least one sub-pattern must match.
    Or(Vec<ExpressionPattern>),

    /// Matches if the inner pattern does NOT match.
    Not(Box<ExpressionPattern>),

    /// Custom predicate for cases where declarative patterns are insufficient.
    Predicate(Arc<dyn Fn(&Expression<'_>) -> bool + Send + Sync>),

    // -- Concrete matchers --
    /// Matches an identifier with the given name.
    Identifier(String),

    /// Matches any identifier.
    AnyIdentifier,

    /// Matches a numeric literal with the given value.
    Number(f64),

    /// Matches any numeric literal.
    AnyNumber,

    /// Matches a string literal with the given value.
    StringLiteral(String),

    /// Matches any string literal.
    AnyStringLiteral,

    /// Matches a boolean literal with the given value.
    Boolean(bool),

    /// Matches a null literal.
    Null,

    /// Matches a binary expression with the given operator and operand patterns.
    BinaryExpression(
        BinaryOperator,
        Box<ExpressionPattern>,
        Box<ExpressionPattern>,
    ),

    /// Matches any binary expression (any operator, any operands).
    AnyBinaryExpression,

    /// Matches a unary expression with the given operator and argument pattern.
    UnaryExpression(UnaryOperator, Box<ExpressionPattern>),

    /// Matches a call expression with the given callee and argument patterns.
    CallExpression(Box<ExpressionPattern>, Vec<ExpressionPattern>),

    /// Matches a member expression with the given object and property patterns.
    MemberExpression(Box<ExpressionPattern>, Box<ExpressionPattern>),

    /// Matches a sequence (comma) expression with the given sub-expression patterns.
    SequenceExpression(Vec<ExpressionPattern>),

    /// Matches a conditional (ternary) expression.
    ConditionalExpression(
        Box<ExpressionPattern>,
        Box<ExpressionPattern>,
        Box<ExpressionPattern>,
    ),

    /// Matches an assignment expression with the given operator and operand patterns.
    AssignmentExpression(
        oxc_syntax::operator::AssignmentOperator,
        Box<ExpressionPattern>,
        Box<ExpressionPattern>,
    ),

    /// Matches an array expression with the given element patterns.
    ArrayExpression(Vec<ExpressionPattern>),
}

impl ExpressionPattern {
    /// Match this pattern against an expression, returning a `MatchResult`
    /// that indicates whether it matched and any captured values.
    pub fn match_expression(&self, expression: &Expression<'_>) -> MatchResult {
        match self {
            // -- Meta-patterns --
            ExpressionPattern::Any => MatchResult::matched(),

            ExpressionPattern::AnyLiteral => {
                if matches!(
                    expression,
                    Expression::NumericLiteral(_)
                        | Expression::StringLiteral(_)
                        | Expression::BooleanLiteral(_)
                        | Expression::NullLiteral(_)
                ) {
                    MatchResult::matched()
                } else {
                    MatchResult::no_match()
                }
            }

            ExpressionPattern::Capture(name, inner) => {
                let mut result = inner.match_expression(expression);
                if result.matched {
                    // Extract a value to capture based on the expression type.
                    if let Some(captured) = extract_capture_value(expression) {
                        result.captures.insert(name.clone(), captured);
                    }
                }
                result
            }

            ExpressionPattern::And(patterns) => {
                let mut combined = MatchResult::matched();
                for pattern in patterns {
                    let result = pattern.match_expression(expression);
                    if !result.matched {
                        return MatchResult::no_match();
                    }
                    combined.merge_captures(&result);
                }
                combined
            }

            ExpressionPattern::Or(patterns) => {
                for pattern in patterns {
                    let result = pattern.match_expression(expression);
                    if result.matched {
                        return result;
                    }
                }
                MatchResult::no_match()
            }

            ExpressionPattern::Not(inner) => {
                if inner.match_expression(expression).matched {
                    MatchResult::no_match()
                } else {
                    MatchResult::matched()
                }
            }

            ExpressionPattern::Predicate(predicate) => {
                if predicate(expression) {
                    MatchResult::matched()
                } else {
                    MatchResult::no_match()
                }
            }

            // -- Concrete matchers --
            ExpressionPattern::Identifier(name) => {
                if let Expression::Identifier(identifier) = expression
                    && identifier.name.as_str() == name {
                        return MatchResult::matched();
                    }
                MatchResult::no_match()
            }

            ExpressionPattern::AnyIdentifier => {
                if matches!(expression, Expression::Identifier(_)) {
                    MatchResult::matched()
                } else {
                    MatchResult::no_match()
                }
            }

            ExpressionPattern::Number(expected) => {
                if let Expression::NumericLiteral(number) = expression
                    && (number.value - expected).abs() < f64::EPSILON {
                        return MatchResult::matched();
                    }
                MatchResult::no_match()
            }

            ExpressionPattern::AnyNumber => {
                if matches!(expression, Expression::NumericLiteral(_)) {
                    MatchResult::matched()
                } else {
                    MatchResult::no_match()
                }
            }

            ExpressionPattern::StringLiteral(expected) => {
                if let Expression::StringLiteral(string) = expression
                    && string.value.as_str() == expected {
                        return MatchResult::matched();
                    }
                MatchResult::no_match()
            }

            ExpressionPattern::AnyStringLiteral => {
                if matches!(expression, Expression::StringLiteral(_)) {
                    MatchResult::matched()
                } else {
                    MatchResult::no_match()
                }
            }

            ExpressionPattern::Boolean(expected) => {
                if let Expression::BooleanLiteral(boolean) = expression
                    && boolean.value == *expected {
                        return MatchResult::matched();
                    }
                MatchResult::no_match()
            }

            ExpressionPattern::Null => {
                if matches!(expression, Expression::NullLiteral(_)) {
                    MatchResult::matched()
                } else {
                    MatchResult::no_match()
                }
            }

            ExpressionPattern::BinaryExpression(operator, left_pattern, right_pattern) => {
                if let Expression::BinaryExpression(binary) = expression
                    && binary.operator == *operator {
                        let left_result = left_pattern.match_expression(&binary.left);
                        if !left_result.matched {
                            return MatchResult::no_match();
                        }
                        let right_result = right_pattern.match_expression(&binary.right);
                        if !right_result.matched {
                            return MatchResult::no_match();
                        }
                        let mut result = MatchResult::matched();
                        result.merge_captures(&left_result);
                        result.merge_captures(&right_result);
                        return result;
                    }
                MatchResult::no_match()
            }

            ExpressionPattern::AnyBinaryExpression => {
                if matches!(expression, Expression::BinaryExpression(_)) {
                    MatchResult::matched()
                } else {
                    MatchResult::no_match()
                }
            }

            ExpressionPattern::UnaryExpression(operator, argument_pattern) => {
                if let Expression::UnaryExpression(unary) = expression
                    && unary.operator == *operator {
                        return argument_pattern.match_expression(&unary.argument);
                    }
                MatchResult::no_match()
            }

            ExpressionPattern::CallExpression(callee_pattern, argument_patterns) => {
                if let Expression::CallExpression(call) = expression {
                    let callee_result = callee_pattern.match_expression(&call.callee);
                    if !callee_result.matched {
                        return MatchResult::no_match();
                    }

                    if call.arguments.len() != argument_patterns.len() {
                        return MatchResult::no_match();
                    }

                    let mut result = MatchResult::matched();
                    result.merge_captures(&callee_result);

                    for (argument, pattern) in call.arguments.iter().zip(argument_patterns) {
                        let Some(argument_expression) = argument.as_expression() else {
                            return MatchResult::no_match();
                        };
                        let argument_result = pattern.match_expression(argument_expression);
                        if !argument_result.matched {
                            return MatchResult::no_match();
                        }
                        result.merge_captures(&argument_result);
                    }

                    return result;
                }
                MatchResult::no_match()
            }

            ExpressionPattern::MemberExpression(object_pattern, property_pattern) => {
                match expression {
                    Expression::StaticMemberExpression(member) => {
                        let object_result = object_pattern.match_expression(&member.object);
                        if !object_result.matched {
                            return MatchResult::no_match();
                        }
                        // For static members, the property is an IdentifierName, not an Expression.
                        // Match against the property name as if it were an identifier pattern.
                        let property_match = match property_pattern.as_ref() {
                            ExpressionPattern::Identifier(name) => {
                                member.property.name.as_str() == name
                            }
                            ExpressionPattern::AnyIdentifier | ExpressionPattern::Any => true,
                            _ => false,
                        };
                        if property_match {
                            let mut result = MatchResult::matched();
                            result.merge_captures(&object_result);
                            return result;
                        }
                        MatchResult::no_match()
                    }
                    Expression::ComputedMemberExpression(member) => {
                        let object_result = object_pattern.match_expression(&member.object);
                        if !object_result.matched {
                            return MatchResult::no_match();
                        }
                        let property_result = property_pattern.match_expression(&member.expression);
                        if !property_result.matched {
                            return MatchResult::no_match();
                        }
                        let mut result = MatchResult::matched();
                        result.merge_captures(&object_result);
                        result.merge_captures(&property_result);
                        result
                    }
                    _ => MatchResult::no_match(),
                }
            }

            ExpressionPattern::SequenceExpression(patterns) => {
                if let Expression::SequenceExpression(sequence) = expression {
                    if sequence.expressions.len() != patterns.len() {
                        return MatchResult::no_match();
                    }
                    let mut result = MatchResult::matched();
                    for (sub_expression, pattern) in sequence.expressions.iter().zip(patterns) {
                        let sub_result = pattern.match_expression(sub_expression);
                        if !sub_result.matched {
                            return MatchResult::no_match();
                        }
                        result.merge_captures(&sub_result);
                    }
                    return result;
                }
                MatchResult::no_match()
            }

            ExpressionPattern::ConditionalExpression(
                test_pattern,
                consequent_pattern,
                alternate_pattern,
            ) => {
                if let Expression::ConditionalExpression(conditional) = expression {
                    let test_result = test_pattern.match_expression(&conditional.test);
                    if !test_result.matched {
                        return MatchResult::no_match();
                    }
                    let consequent_result =
                        consequent_pattern.match_expression(&conditional.consequent);
                    if !consequent_result.matched {
                        return MatchResult::no_match();
                    }
                    let alternate_result =
                        alternate_pattern.match_expression(&conditional.alternate);
                    if !alternate_result.matched {
                        return MatchResult::no_match();
                    }
                    let mut result = MatchResult::matched();
                    result.merge_captures(&test_result);
                    result.merge_captures(&consequent_result);
                    result.merge_captures(&alternate_result);
                    return result;
                }
                MatchResult::no_match()
            }

            ExpressionPattern::AssignmentExpression(operator, left_pattern, right_pattern) => {
                if let Expression::AssignmentExpression(assignment) = expression
                    && assignment.operator == *operator {
                        // The left side of an assignment is an AssignmentTarget, not an Expression.
                        // For simple identifier targets, we can match against the name.
                        let left_match = match (&assignment.left, left_pattern.as_ref()) {
                            (
                                oxc_ast::ast::AssignmentTarget::AssignmentTargetIdentifier(
                                    identifier,
                                ),
                                ExpressionPattern::Identifier(name),
                            ) => identifier.name.as_str() == name,
                            (
                                oxc_ast::ast::AssignmentTarget::AssignmentTargetIdentifier(_),
                                ExpressionPattern::AnyIdentifier | ExpressionPattern::Any,
                            ) => true,
                            _ => false,
                        };

                        if left_match {
                            let right_result = right_pattern.match_expression(&assignment.right);
                            if right_result.matched {
                                return right_result;
                            }
                        }
                    }
                MatchResult::no_match()
            }

            ExpressionPattern::ArrayExpression(element_patterns) => {
                if let Expression::ArrayExpression(array) = expression {
                    if array.elements.len() != element_patterns.len() {
                        return MatchResult::no_match();
                    }
                    let mut result = MatchResult::matched();
                    for (element, pattern) in array.elements.iter().zip(element_patterns) {
                        let element_expression = match element {
                            oxc_ast::ast::ArrayExpressionElement::SpreadElement(_) => {
                                return MatchResult::no_match();
                            }
                            oxc_ast::ast::ArrayExpressionElement::Elision(_) => {
                                return MatchResult::no_match();
                            }
                            _ => element.to_expression(),
                        };
                        let element_result = pattern.match_expression(element_expression);
                        if !element_result.matched {
                            return MatchResult::no_match();
                        }
                        result.merge_captures(&element_result);
                    }
                    return result;
                }
                MatchResult::no_match()
            }
        }
    }
}

/// Extract a capturable value from an expression.
fn extract_capture_value(expression: &Expression<'_>) -> Option<CapturedNode> {
    match expression {
        Expression::Identifier(identifier) => {
            Some(CapturedNode::StringValue(identifier.name.to_string()))
        }
        Expression::StringLiteral(string) => {
            Some(CapturedNode::StringValue(string.value.to_string()))
        }
        Expression::NumericLiteral(number) => Some(CapturedNode::NumberValue(number.value)),
        Expression::BooleanLiteral(boolean) => Some(CapturedNode::BooleanValue(boolean.value)),
        _ => None,
    }
}
