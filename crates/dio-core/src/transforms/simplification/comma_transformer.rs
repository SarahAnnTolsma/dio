//! Simplifies sequence (comma) expressions.
//!
//! `(1, 2, x)` -> `x` when only the last value matters.
//! Only simplifies when all expressions except the last are side-effect-free literals.

use oxc_ast::ast::Expression;
use oxc_traverse::TraverseCtx;

use crate::operations;
use crate::transformer::{AstNodeType, Transformer, TransformerPhase, TransformerPriority};

/// Simplifies sequence (comma) expressions by removing side-effect-free leading expressions.
pub struct CommaTransformer;

impl Transformer for CommaTransformer {
    fn name(&self) -> &str {
        "CommaTransformer"
    }

    fn interests(&self) -> &[AstNodeType] {
        &[AstNodeType::SequenceExpression]
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
        let Expression::SequenceExpression(sequence) = expression else {
            return false;
        };

        if sequence.expressions.len() <= 1 {
            return false;
        }

        // Check if all expressions except the last are side-effect-free.
        let all_leading_pure = sequence.expressions[..sequence.expressions.len() - 1]
            .iter()
            .all(is_side_effect_free);

        if !all_leading_pure {
            return false;
        }

        // Replace the entire sequence with just the last expression.
        let last = sequence.expressions.pop().unwrap();
        operations::replace_expression(expression, last, context);
        true
    }
}

/// Check if an expression is definitely side-effect-free.
/// Conservative: only returns true for literals and identifiers.
fn is_side_effect_free(expression: &Expression<'_>) -> bool {
    matches!(
        expression,
        Expression::NumericLiteral(_)
            | Expression::StringLiteral(_)
            | Expression::BooleanLiteral(_)
            | Expression::NullLiteral(_)
            | Expression::Identifier(_)
    )
}
