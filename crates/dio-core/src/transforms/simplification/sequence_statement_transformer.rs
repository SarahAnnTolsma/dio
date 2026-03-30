//! Extracts leading expressions from sequence expressions in statement contexts.
//!
//! When a sequence (comma) expression is used in a position where only the last
//! value matters, the leading expressions are hoisted as standalone statements:
//!
//! - `return (a, b, c);` -> `a; b; return c;`
//! - `if (a, b, c) { ... }` -> `a; b; if (c) { ... }`
//!
//! This complements the `CommaTransformer`, which drops side-effect-free leading
//! expressions. This transformer preserves all leading expressions as statements,
//! regardless of whether they have side effects.

use oxc_allocator::Vec as ArenaVec;
use oxc_ast::ast::{Expression, Statement};
use oxc_span::SPAN;
use oxc_traverse::TraverseCtx;

use crate::transformer::{AstNodeType, Transformer, TransformerPhase, TransformerPriority};

/// Hoists leading expressions from sequence expressions in return and if statements.
pub struct SequenceStatementTransformer;

impl Transformer for SequenceStatementTransformer {
    fn name(&self) -> &str {
        "SequenceStatementTransformer"
    }

    fn interests(&self) -> &[AstNodeType] {
        &[AstNodeType::StatementList]
    }

    fn priority(&self) -> TransformerPriority {
        TransformerPriority::Default
    }

    fn phase(&self) -> TransformerPhase {
        TransformerPhase::Main
    }

    fn enter_statements<'a>(
        &self,
        statements: &mut ArenaVec<'a, Statement<'a>>,
        context: &mut TraverseCtx<'a, ()>,
    ) -> bool {
        let mut changed = false;
        let mut index = 0;

        while index < statements.len() {
            if let Some(extracted) = try_extract_leading_expressions(&mut statements[index], context)
            {
                let count = extracted.len();
                // Insert extracted statements before the current statement.
                for (offset, extracted_statement) in extracted.into_iter().enumerate() {
                    statements.insert(index + offset, extracted_statement);
                }
                // Skip past the inserted statements and the modified original.
                index += count + 1;
                changed = true;
            } else {
                index += 1;
            }
        }

        changed
    }
}

/// If the statement contains a sequence expression in a qualifying position,
/// extract the leading expressions as expression statements and mutate the
/// original statement to use only the last expression. Returns the extracted
/// statements, or `None` if this statement doesn't qualify.
fn try_extract_leading_expressions<'a>(
    statement: &mut Statement<'a>,
    context: &mut TraverseCtx<'a, ()>,
) -> Option<Vec<Statement<'a>>> {
    let target_expression = match statement {
        Statement::ReturnStatement(return_statement) => return_statement.argument.as_mut()?,
        Statement::IfStatement(if_statement) => &mut if_statement.test,
        _ => return None,
    };

    // Unwrap parenthesized expressions to find the sequence.
    let inner = unwrap_parens_mut(target_expression);
    if !matches!(inner, Expression::SequenceExpression(seq) if seq.expressions.len() > 1) {
        return None;
    }

    let Expression::SequenceExpression(sequence) = inner else {
        unreachable!();
    };

    // Pop the last expression — it stays in the original statement.
    let last = sequence.expressions.pop().unwrap();

    // Drain all remaining expressions (the leading ones) into standalone statements.
    let mut extracted = Vec::new();
    for expression in sequence.expressions.drain(..) {
        extracted.push(context.ast.statement_expression(SPAN, expression));
    }

    // Replace the entire expression (including any ParenthesizedExpression wrapper)
    // with just the last expression.
    *target_expression = last;

    Some(extracted)
}

/// Unwrap parenthesized expressions to get a mutable reference to the inner expression.
fn unwrap_parens_mut<'a, 'b>(expression: &'b mut Expression<'a>) -> &'b mut Expression<'a> {
    let mut current = expression;
    while let Expression::ParenthesizedExpression(paren) = current {
        current = &mut paren.expression;
    }
    current
}
