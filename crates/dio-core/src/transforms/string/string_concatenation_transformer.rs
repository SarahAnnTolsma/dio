//! Concatenates adjacent string literals in binary addition chains.
//!
//! Handles left-associative chaining: `("a" + "b") + "c"` -> `"abc"`

use oxc_ast::ast::Expression;
use oxc_span::SPAN;
use oxc_syntax::operator::BinaryOperator;
use oxc_traverse::TraverseCtx;

use crate::transformer::{AstNodeType, Transformer, TransformerPhase, TransformerPriority};

/// Concatenates chains of string literal additions into a single string.
pub struct StringConcatenationTransformer;

impl Transformer for StringConcatenationTransformer {
    fn name(&self) -> &str {
        "StringConcatenationTransformer"
    }

    fn interests(&self) -> &[AstNodeType] {
        &[AstNodeType::BinaryExpression]
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
        let Expression::BinaryExpression(binary) = expression else {
            return false;
        };

        if binary.operator != BinaryOperator::Addition {
            return false;
        }

        // Collect all string parts from a left-associative chain of additions.
        let mut parts = Vec::new();
        if collect_string_parts(&binary.left, &mut parts)
            && collect_string_parts(&binary.right, &mut parts)
            && parts.len() >= 2
        {
            let concatenated: String = parts.join("");
            let value = context.ast.atom(&concatenated);
            *expression = context.ast.expression_string_literal(SPAN, value, None);
            return true;
        }

        false
    }
}

/// Recursively collect string literal parts from a chain of `+` operations.
fn collect_string_parts(expression: &Expression<'_>, parts: &mut Vec<String>) -> bool {
    match expression {
        Expression::StringLiteral(literal) => {
            parts.push(literal.value.to_string());
            true
        }
        Expression::BinaryExpression(binary) if binary.operator == BinaryOperator::Addition => {
            collect_string_parts(&binary.left, parts) && collect_string_parts(&binary.right, parts)
        }
        _ => false,
    }
}
