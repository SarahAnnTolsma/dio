//! Simplifies control flow based on constant conditions.
//!
//! - `if (true) A else B` -> `A`
//! - `if (false) A else B` -> `B` (or removes entirely if no else)
//! - `condition ? consequent : alternate` with boolean condition -> the appropriate branch

use oxc_ast::ast::{Expression, Statement};
use oxc_span::SPAN;
use oxc_traverse::TraverseCtx;

use crate::operations;
use crate::transformer::{AstNodeType, Transformer, TransformerPhase, TransformerPriority};
use crate::utils::unwrap_parens;

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

        // `cond ? true : false` → `cond`
        // `cond ? false : true` → `!cond`
        let consequent_bool = evaluate_as_boolean(&conditional.consequent);
        let alternate_bool = evaluate_as_boolean(&conditional.alternate);
        if let (Some(true), Some(false)) = (consequent_bool, alternate_bool) {
            let test = std::mem::replace(
                &mut conditional.test,
                context.ast.expression_null_literal(SPAN),
            );
            operations::replace_expression(expression, test, context);
            return true;
        }
        if let (Some(false), Some(true)) = (consequent_bool, alternate_bool) {
            let test = std::mem::replace(
                &mut conditional.test,
                context.ast.expression_null_literal(SPAN),
            );
            let negated = context.ast.expression_unary(
                SPAN,
                oxc_syntax::operator::UnaryOperator::LogicalNot,
                test,
            );
            operations::replace_expression(expression, negated, context);
            return true;
        }

        let Some(is_truthy) = evaluate_as_boolean(&conditional.test) else {
            return false;
        };

        if is_truthy {
            let consequent = std::mem::replace(
                &mut conditional.consequent,
                context.ast.expression_null_literal(SPAN),
            );
            operations::replace_expression(expression, consequent, context);
        } else {
            let alternate = std::mem::replace(
                &mut conditional.alternate,
                context.ast.expression_null_literal(SPAN),
            );
            operations::replace_expression(expression, alternate, context);
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

        // Constant condition — replace with the taken branch.
        if let Some(is_truthy) = evaluate_as_boolean(&if_statement.test) {
            if is_truthy {
                let consequent = std::mem::replace(
                    &mut if_statement.consequent,
                    context.ast.statement_empty(SPAN),
                );
                let unwrapped = unwrap_single_statement_block(consequent);
                operations::replace_statement(statement, unwrapped, context);
            } else if let Some(alternate) = &mut if_statement.alternate {
                let alternate = std::mem::replace(alternate, context.ast.statement_empty(SPAN));
                let unwrapped = unwrap_single_statement_block(alternate);
                operations::replace_statement(statement, unwrapped, context);
            } else {
                operations::remove_statement(statement, context);
            }
            return true;
        }

        // Empty consequent and alternate — remove the if entirely, keep the
        // test as an expression statement in case it has side effects.
        let consequent_empty = is_empty_body(&if_statement.consequent);
        let alternate_empty = if_statement
            .alternate
            .as_ref()
            .map_or(true, |alternate| is_empty_body(alternate));

        if consequent_empty && alternate_empty {
            // Replace `if (test) {} else {}` with `test;`
            let test = std::mem::replace(
                &mut if_statement.test,
                context.ast.expression_null_literal(SPAN),
            );
            let replacement = context.ast.statement_expression(SPAN, test);
            operations::replace_statement(statement, replacement, context);
            return true;
        }

        // Empty alternate — remove the else branch.
        if !consequent_empty && !alternate_empty {
            // Both non-empty, nothing to simplify.
            return false;
        }

        if !consequent_empty && alternate_empty && if_statement.alternate.is_some() {
            // `if (test) { body } else {}` → `if (test) { body }`
            let Statement::IfStatement(if_statement) = statement else {
                return false;
            };
            if_statement.alternate = None;
            return true;
        }

        if consequent_empty && !alternate_empty {
            // `if (test) {} else { body }` → `if (!test) { body }`
            let Statement::IfStatement(if_statement) = statement else {
                return false;
            };
            let test = std::mem::replace(
                &mut if_statement.test,
                context.ast.expression_null_literal(SPAN),
            );
            let negated = context.ast.expression_unary(
                SPAN,
                oxc_syntax::operator::UnaryOperator::LogicalNot,
                test,
            );
            if_statement.test = negated;
            let alternate = if_statement.alternate.take().unwrap();
            if_statement.consequent = alternate;
            return true;
        }

        false
    }
}

/// Check if a statement body is effectively empty (empty block or empty statement).
fn is_empty_body(statement: &Statement<'_>) -> bool {
    match statement {
        Statement::EmptyStatement(_) => true,
        Statement::BlockStatement(block) => block.body.is_empty(),
        _ => false,
    }
}

/// Unwrap a block statement containing a single statement to just that statement.
/// `{ x = 1; }` -> `x = 1;`
fn unwrap_single_statement_block<'a>(statement: Statement<'a>) -> Statement<'a> {
    if let Statement::BlockStatement(mut block) = statement {
        if block.body.len() == 1 {
            return block.body.pop().unwrap();
        }
        Statement::BlockStatement(block)
    } else {
        statement
    }
}

/// Try to evaluate an expression as a known boolean value.
/// Looks through parenthesized expressions.
fn evaluate_as_boolean(expression: &Expression<'_>) -> Option<bool> {
    let expression = unwrap_parens(expression);
    match expression {
        Expression::BooleanLiteral(boolean) => Some(boolean.value),
        Expression::NumericLiteral(number) => Some(number.value != 0.0 && !number.value.is_nan()),
        Expression::StringLiteral(string) => Some(!string.value.is_empty()),
        Expression::NullLiteral(_) => Some(false),
        _ => None,
    }
}

