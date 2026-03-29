//! Ergonomic builder functions for constructing patterns.
//!
//! These free functions provide a concise way to build `ExpressionPattern`
//! and `StatementPattern` values without verbose enum construction.

use oxc_syntax::operator::{AssignmentOperator, BinaryOperator, UnaryOperator};

use super::expression::ExpressionPattern;
use super::statement::StatementPattern;

// -- Expression pattern builders --

/// Match any expression.
pub fn any() -> ExpressionPattern {
    ExpressionPattern::Any
}

/// Match any literal expression (number, string, boolean, null).
pub fn any_literal() -> ExpressionPattern {
    ExpressionPattern::AnyLiteral
}

/// Match and capture the inner pattern's value under the given name.
pub fn capture(name: &str, inner: ExpressionPattern) -> ExpressionPattern {
    ExpressionPattern::Capture(name.to_string(), Box::new(inner))
}

/// Match an identifier with the exact given name.
pub fn identifier(name: &str) -> ExpressionPattern {
    ExpressionPattern::Identifier(name.to_string())
}

/// Match any identifier.
pub fn any_identifier() -> ExpressionPattern {
    ExpressionPattern::AnyIdentifier
}

/// Match a numeric literal with the exact given value.
pub fn number(value: f64) -> ExpressionPattern {
    ExpressionPattern::Number(value)
}

/// Match any numeric literal.
pub fn any_number() -> ExpressionPattern {
    ExpressionPattern::AnyNumber
}

/// Match a string literal with the exact given value.
pub fn string_literal(value: &str) -> ExpressionPattern {
    ExpressionPattern::StringLiteral(value.to_string())
}

/// Match any string literal.
pub fn any_string_literal() -> ExpressionPattern {
    ExpressionPattern::AnyStringLiteral
}

/// Match a boolean literal with the given value.
pub fn boolean(value: bool) -> ExpressionPattern {
    ExpressionPattern::Boolean(value)
}

/// Match a null literal.
pub fn null() -> ExpressionPattern {
    ExpressionPattern::Null
}

/// Match a binary expression with the given operator and operand patterns.
pub fn binary_expression(
    operator: BinaryOperator,
    left: ExpressionPattern,
    right: ExpressionPattern,
) -> ExpressionPattern {
    ExpressionPattern::BinaryExpression(operator, Box::new(left), Box::new(right))
}

/// Match any binary expression.
pub fn any_binary_expression() -> ExpressionPattern {
    ExpressionPattern::AnyBinaryExpression
}

/// Match a unary expression with the given operator and argument pattern.
pub fn unary_expression(operator: UnaryOperator, argument: ExpressionPattern) -> ExpressionPattern {
    ExpressionPattern::UnaryExpression(operator, Box::new(argument))
}

/// Match a call expression with the given callee and argument patterns.
pub fn call_expression(
    callee: ExpressionPattern,
    arguments: Vec<ExpressionPattern>,
) -> ExpressionPattern {
    ExpressionPattern::CallExpression(Box::new(callee), arguments)
}

/// Match a member expression with the given object and property patterns.
pub fn member_expression(
    object: ExpressionPattern,
    property: ExpressionPattern,
) -> ExpressionPattern {
    ExpressionPattern::MemberExpression(Box::new(object), Box::new(property))
}

/// Match a sequence (comma) expression with the given sub-expression patterns.
pub fn sequence_expression(expressions: Vec<ExpressionPattern>) -> ExpressionPattern {
    ExpressionPattern::SequenceExpression(expressions)
}

/// Match a conditional (ternary) expression.
pub fn conditional_expression(
    test: ExpressionPattern,
    consequent: ExpressionPattern,
    alternate: ExpressionPattern,
) -> ExpressionPattern {
    ExpressionPattern::ConditionalExpression(
        Box::new(test),
        Box::new(consequent),
        Box::new(alternate),
    )
}

/// Match an assignment expression.
pub fn assignment_expression(
    operator: AssignmentOperator,
    left: ExpressionPattern,
    right: ExpressionPattern,
) -> ExpressionPattern {
    ExpressionPattern::AssignmentExpression(operator, Box::new(left), Box::new(right))
}

/// Match an array expression with the given element patterns.
pub fn array_expression(elements: Vec<ExpressionPattern>) -> ExpressionPattern {
    ExpressionPattern::ArrayExpression(elements)
}

/// All sub-patterns must match the same expression.
pub fn and(patterns: Vec<ExpressionPattern>) -> ExpressionPattern {
    ExpressionPattern::And(patterns)
}

/// At least one sub-pattern must match.
pub fn or(patterns: Vec<ExpressionPattern>) -> ExpressionPattern {
    ExpressionPattern::Or(patterns)
}

/// Matches if the inner pattern does NOT match.
pub fn not(pattern: ExpressionPattern) -> ExpressionPattern {
    ExpressionPattern::Not(Box::new(pattern))
}

// -- Statement pattern builders --

/// Match any statement.
pub fn any_statement() -> StatementPattern {
    StatementPattern::Any
}

/// Match and capture a statement.
pub fn capture_statement(name: &str, inner: StatementPattern) -> StatementPattern {
    StatementPattern::Capture(name.to_string(), Box::new(inner))
}

/// Match zero or more consecutive statements matching the inner pattern.
pub fn repeat(inner: StatementPattern) -> StatementPattern {
    StatementPattern::Repeat(Box::new(inner))
}

/// Match an expression statement.
pub fn expression_statement(expression: ExpressionPattern) -> StatementPattern {
    StatementPattern::ExpressionStatement(expression)
}

/// Match a return statement with an optional value pattern.
pub fn return_statement(value: Option<ExpressionPattern>) -> StatementPattern {
    StatementPattern::ReturnStatement(value)
}

/// Match a block statement with the given body patterns.
pub fn block_statement(body: Vec<StatementPattern>) -> StatementPattern {
    StatementPattern::BlockStatement(body)
}

/// Match an if statement.
pub fn if_statement(
    test: ExpressionPattern,
    consequent: StatementPattern,
    alternate: Option<StatementPattern>,
) -> StatementPattern {
    StatementPattern::IfStatement {
        test,
        consequent: Box::new(consequent),
        alternate: alternate.map(Box::new),
    }
}

/// Match a variable declaration.
pub fn variable_declaration() -> StatementPattern {
    StatementPattern::VariableDeclaration
}

/// Match an empty statement.
pub fn empty_statement() -> StatementPattern {
    StatementPattern::EmptyStatement
}
