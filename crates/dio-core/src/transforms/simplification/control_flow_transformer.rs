//! Simplifies control flow based on constant conditions.
//!
//! - `if (true) A else B` -> `A`
//! - `if (false) A else B` -> `B` (or removes entirely if no else)
//! - `condition ? consequent : alternate` with boolean condition -> the appropriate branch

use oxc_ast::ast::{Expression, Statement};
use oxc_span::SPAN;
use oxc_traverse::TraverseCtx;

use crate::transformer::{AstNodeType, Transformer, TransformerPhase, TransformerPriority};

/// Simplifies control flow when conditions are constant boolean values.
pub struct ControlFlowTransformer;

impl Transformer for ControlFlowTransformer {
    fn name(&self) -> &str {
        "ControlFlowTransformer"
    }

    fn interests(&self) -> &[AstNodeType] {
        &[AstNodeType::ConditionalExpression, AstNodeType::IfStatement]
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
        let Expression::ConditionalExpression(conditional) = expression else {
            return false;
        };

        let Some(is_truthy) = evaluate_as_boolean(&conditional.test) else {
            return false;
        };

        if is_truthy {
            let consequent = std::mem::replace(
                &mut conditional.consequent,
                context.ast.expression_null_literal(SPAN),
            );
            *expression = consequent;
        } else {
            let alternate = std::mem::replace(
                &mut conditional.alternate,
                context.ast.expression_null_literal(SPAN),
            );
            *expression = alternate;
        }

        true
    }

    fn enter_statement<'a>(
        &self,
        statement: &mut Statement<'a>,
        context: &mut TraverseCtx<'a, ()>,
    ) -> bool {
        let Statement::IfStatement(if_statement) = statement else {
            return false;
        };

        let Some(is_truthy) = evaluate_as_boolean(&if_statement.test) else {
            return false;
        };

        if is_truthy {
            let consequent = std::mem::replace(
                &mut if_statement.consequent,
                context.ast.statement_empty(SPAN),
            );
            *statement = consequent;
        } else if let Some(alternate) = &mut if_statement.alternate {
            let alternate = std::mem::replace(alternate, context.ast.statement_empty(SPAN));
            *statement = alternate;
        } else {
            *statement = context.ast.statement_empty(SPAN);
        }

        true
    }
}

/// Try to evaluate an expression as a known boolean value.
fn evaluate_as_boolean(expression: &Expression<'_>) -> Option<bool> {
    match expression {
        Expression::BooleanLiteral(boolean) => Some(boolean.value),
        Expression::NumericLiteral(number) => Some(number.value != 0.0 && !number.value.is_nan()),
        Expression::StringLiteral(string) => Some(!string.value.is_empty()),
        Expression::NullLiteral(_) => Some(false),
        _ => None,
    }
}
