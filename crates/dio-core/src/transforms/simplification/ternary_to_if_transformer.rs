//! Converts standalone ternary expressions into if/else statements.
//!
//! `x ? y() : z();` -> `if (x) { y(); } else { z(); }`
//!
//! Only applies when the ternary is the entire expression statement — not when
//! it is used as a value (e.g., `var a = x ? y : z;`).

use oxc_ast::ast::{Expression, Statement};
use oxc_span::SPAN;
use oxc_traverse::TraverseCtx;

use crate::operations;
use crate::transformer::{AstNodeType, Transformer, TransformerPhase, TransformerPriority};

/// Converts expression statements containing a single ternary into if/else.
pub struct TernaryToIfTransformer;

impl Transformer for TernaryToIfTransformer {
    fn name(&self) -> &str {
        "TernaryToIfTransformer"
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
            Expression::ConditionalExpression(_)
        ) {
            return false;
        }

        // Take the conditional expression out of the expression statement.
        let expression = std::mem::replace(
            &mut expression_statement.expression,
            context.ast.expression_null_literal(SPAN),
        );
        let Expression::ConditionalExpression(conditional) = expression else {
            unreachable!();
        };

        let conditional = conditional.unbox();
        let test = conditional.test;
        let consequent = conditional.consequent;
        let alternate = conditional.alternate;

        // Build the consequent and alternate as expression statements inside blocks.
        let consequent_statement = context.ast.statement_expression(SPAN, consequent);
        let consequent_body = context.ast.vec_from_array([consequent_statement]);
        let consequent_block = operations::create_block_statement(consequent_body, context);

        let alternate_statement = context.ast.statement_expression(SPAN, alternate);
        let alternate_body = context.ast.vec_from_array([alternate_statement]);
        let alternate_block = operations::create_block_statement(alternate_body, context);

        let replacement =
            context
                .ast
                .statement_if(SPAN, test, consequent_block, Some(alternate_block));

        operations::replace_statement(statement, replacement, context);
        true
    }
}
