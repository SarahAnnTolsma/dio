//! Unwraps `setTimeout` calls that immediately assign a constant to a variable.
//!
//! DataDome uses `setTimeout(function() { x = value; }, 0)` to defer simple
//! variable assignments. Since the 0ms timeout executes synchronously before
//! any I/O, we can safely inline these as direct assignments.
//!
//! # Example
//!
//! ```js
//! // Before
//! setTimeout(function() {
//!     p = -418;
//! }, 0);
//!
//! // After
//! p = -418;
//! ```

use oxc_ast::ast::{Expression, Statement};
use oxc_traverse::TraverseCtx;

use crate::operations;
use crate::transformer::{AstNodeType, Transformer, TransformerPhase, TransformerPriority};

/// Unwraps `setTimeout(function() { assignment; }, 0)` into the bare assignment.
pub struct SetTimeoutUnwrapTransformer;

impl Transformer for SetTimeoutUnwrapTransformer {
    fn name(&self) -> &str {
        "SetTimeoutUnwrapTransformer"
    }

    fn interests(&self) -> &[AstNodeType] {
        &[AstNodeType::ExpressionStatement]
    }

    fn priority(&self) -> TransformerPriority {
        TransformerPriority::First
    }

    fn phase(&self) -> TransformerPhase {
        TransformerPhase::Main
    }

    fn enter_statement<'a>(
        &self,
        statement: &mut Statement<'a>,
        context: &mut TraverseCtx<'a, ()>,
    ) -> bool {
        let Statement::ExpressionStatement(expression_statement) = statement else {
            return false;
        };

        let Expression::CallExpression(call) = &expression_statement.expression else {
            return false;
        };

        // Callee must be `setTimeout`.
        let Expression::Identifier(callee) = &call.callee else {
            return false;
        };
        if callee.name.as_str() != "setTimeout" {
            return false;
        }

        // Must have exactly 2 arguments.
        if call.arguments.len() != 2 {
            return false;
        }

        // Second argument must be 0.
        let Some(delay_expression) = call.arguments[1].as_expression() else {
            return false;
        };
        let Expression::NumericLiteral(delay) = delay_expression else {
            return false;
        };
        if delay.value != 0.0 {
            return false;
        }

        // First argument must be a function expression with no parameters.
        let Some(callback_expression) = call.arguments[0].as_expression() else {
            return false;
        };
        let Expression::FunctionExpression(function) = callback_expression else {
            return false;
        };
        if !function.params.items.is_empty() {
            return false;
        }

        let Some(body) = &function.body else {
            return false;
        };

        // Body must contain exactly one statement: an expression statement
        // with an assignment of a literal to a variable.
        if body.statements.len() != 1 {
            return false;
        }

        let Statement::ExpressionStatement(inner) = &body.statements[0] else {
            return false;
        };
        let Expression::AssignmentExpression(assignment) = &inner.expression else {
            return false;
        };
        if assignment.operator != oxc_syntax::operator::AssignmentOperator::Assign {
            return false;
        }

        // The right side must be a literal value.
        if !is_literal(&assignment.right) {
            return false;
        }

        // Replace the setTimeout call with the inner assignment statement.
        // We need to take the inner expression out of the callback body.
        let Statement::ExpressionStatement(expression_statement) = statement else {
            return false;
        };
        let Expression::CallExpression(call) = &mut expression_statement.expression else {
            return false;
        };
        let Some(callback_expression) = call.arguments[0].as_expression_mut() else {
            return false;
        };
        let Expression::FunctionExpression(function) = callback_expression else {
            return false;
        };
        let Some(body) = &mut function.body else {
            return false;
        };

        let inner_statement = body.statements.pop().unwrap();
        operations::replace_statement(statement, inner_statement, context);
        true
    }
}

/// Check if an expression is a simple literal (numeric, string, boolean, null).
fn is_literal(expression: &Expression<'_>) -> bool {
    match expression {
        Expression::NumericLiteral(_)
        | Expression::StringLiteral(_)
        | Expression::BooleanLiteral(_)
        | Expression::NullLiteral(_) => true,
        Expression::UnaryExpression(unary) => {
            unary.operator == oxc_syntax::operator::UnaryOperator::UnaryNegation
                && matches!(&unary.argument, Expression::NumericLiteral(_))
        }
        _ => false,
    }
}
