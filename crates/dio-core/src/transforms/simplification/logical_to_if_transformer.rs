//! Converts standalone logical expressions used as statements into if statements.
//!
//! `x && y()` as a statement -> `if (x) { y(); }`
//! `x || y()` as a statement -> `if (!x) { y(); }`
//!
//! Only applies when the logical expression is the entire expression statement — not
//! when it is used as a value (e.g., `var z = x && y`).

use oxc_ast::ast::{Expression, Statement};
use oxc_span::SPAN;
use oxc_syntax::operator::LogicalOperator;
use oxc_traverse::TraverseCtx;

use crate::operations;
use crate::transformer::{AstNodeType, Transformer, TransformerPhase, TransformerPriority};

/// Converts expression statements containing a logical `&&` or `||` into if statements.
pub struct LogicalToIfTransformer;

impl Transformer for LogicalToIfTransformer {
    fn name(&self) -> &str {
        "LogicalToIfTransformer"
    }

    fn interests(&self) -> &[AstNodeType] {
        &[AstNodeType::ExpressionStatement]
    }

    fn priority(&self) -> TransformerPriority {
        TransformerPriority::Default
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

        if !matches!(
            expression_statement.expression,
            Expression::LogicalExpression(_)
        ) {
            return false;
        }

        // Take the logical expression out of the expression statement.
        let expression = std::mem::replace(
            &mut expression_statement.expression,
            context.ast.expression_null_literal(SPAN),
        );
        let Expression::LogicalExpression(logical) = expression else {
            unreachable!();
        };

        let logical = logical.unbox();
        let operator = logical.operator;
        let left = logical.left;
        let right = logical.right;

        let test = match operator {
            LogicalOperator::And => left,
            LogicalOperator::Or => {
                // Wrap the left side in a unary `!` expression.
                context.ast.expression_unary(
                    SPAN,
                    oxc_syntax::operator::UnaryOperator::LogicalNot,
                    left,
                )
            }
            _ => return false,
        };

        // Build the consequent as an expression statement inside a block.
        let consequent_statement = context.ast.statement_expression(SPAN, right);
        let consequent_body = context.ast.vec_from_array([consequent_statement]);
        let consequent_block = operations::create_block_statement(consequent_body, context);

        let replacement = context.ast.statement_if(SPAN, test, consequent_block, None);

        operations::replace_statement(statement, replacement, context);
        true
    }
}
