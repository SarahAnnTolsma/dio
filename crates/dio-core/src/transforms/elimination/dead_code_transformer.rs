//! Removes dead (unreachable) code from statement lists.
//!
//! Handles:
//! - Code after `return`, `throw`, `break`, `continue` in any statement list.
//! - Expression statements with no side effects (numeric, boolean, null,
//!   undefined, and non-directive string literals).
//! - Empty statements.
//!
//! This runs in the Finalize phase so that other transforms have a chance
//! to simplify conditions first.
//!
//! Uses `enter_statements` (via `StatementList` interest) to operate on all
//! statement lists — block bodies, function bodies, program bodies, etc.

use oxc_allocator::Vec as ArenaVec;
use oxc_ast::ast::{Expression, Statement};
use oxc_span::SPAN;
use oxc_traverse::TraverseCtx;

use crate::operations;
use crate::transformer::{AstNodeType, Transformer, TransformerPhase, TransformerPriority};

/// Removes unreachable code and side-effect-free statements.
pub struct DeadCodeTransformer;

impl Transformer for DeadCodeTransformer {
    fn name(&self) -> &str {
        "DeadCodeTransformer"
    }

    fn interests(&self) -> &[AstNodeType] {
        &[AstNodeType::StatementList]
    }

    fn priority(&self) -> TransformerPriority {
        TransformerPriority::Default
    }

    fn phase(&self) -> TransformerPhase {
        TransformerPhase::Finalize
    }

    fn enter_statements<'a>(
        &self,
        statements: &mut ArenaVec<'a, Statement<'a>>,
        context: &mut TraverseCtx<'a, ()>,
    ) -> bool {
        let original_length = statements.len();

        // Find the first terminal statement and remove everything after it,
        // EXCEPT function declarations which are hoisted in JavaScript.
        if let Some(terminal_index) = find_first_terminal(statements) {
            for index in (terminal_index + 1..statements.len()).rev() {
                // Function declarations are hoisted — they must be kept even
                // after return/throw/break/continue.
                if matches!(&statements[index], Statement::FunctionDeclaration(_)) {
                    continue;
                }
                operations::remove_statement_at(statements, index, context);
            }
        }

        // Remove side-effect-free expression statements.
        for index in (0..statements.len()).rev() {
            if is_side_effect_free_statement(&statements[index]) {
                operations::remove_statement_at(statements, index, context);
            }
        }

        // Unwrap bare block statements: `{ a; b; }` → `a; b;` when the
        // block appears as a direct child of a statement list and contains
        // no block-scoped declarations (let/const). These are left over
        // from control flow flattening. `var` is function-scoped and already
        // leaks, so it's safe to promote.
        let mut block_index = 0;
        while block_index < statements.len() {
            if let Statement::BlockStatement(block) = &statements[block_index] {
                if block.body.is_empty() || has_block_scoped_declarations(&block.body) {
                    block_index += 1;
                    continue;
                }
                let inner_count = block.body.len();
                let Statement::BlockStatement(mut block) = std::mem::replace(
                    &mut statements[block_index],
                    context.ast.statement_empty(SPAN),
                ) else {
                    unreachable!();
                };
                let inner: Vec<Statement<'a>> = block.body.drain(..).collect();
                statements.remove(block_index);
                for (offset, stmt) in inner.into_iter().enumerate() {
                    statements.insert(block_index + offset, stmt);
                }
                block_index += inner_count;
            } else {
                block_index += 1;
            }
        }

        // Remove empty statements (clean up references first via operations).
        operations::retain_statements(
            statements,
            |statement| !matches!(statement, Statement::EmptyStatement(_)),
            context,
        );

        // Remove the empty statement placeholders that retain_statements left behind.
        statements.retain(|statement| !matches!(statement, Statement::EmptyStatement(_)));

        statements.len() != original_length
    }
}

/// Find the index of the first terminal statement (return, throw, break, continue).
fn find_first_terminal(statements: &[Statement<'_>]) -> Option<usize> {
    statements.iter().position(is_terminal_statement)
}

/// Check if a statement unconditionally terminates control flow.
fn is_terminal_statement(statement: &Statement<'_>) -> bool {
    matches!(
        statement,
        Statement::ReturnStatement(_)
            | Statement::ThrowStatement(_)
            | Statement::BreakStatement(_)
            | Statement::ContinueStatement(_)
    )
}

/// Check if a statement list contains any block-scoped declarations (let/const).
fn has_block_scoped_declarations(statements: &[Statement<'_>]) -> bool {
    statements.iter().any(|statement| {
        if let Statement::VariableDeclaration(declaration) = statement {
            matches!(
                declaration.kind,
                oxc_ast::ast::VariableDeclarationKind::Let
                    | oxc_ast::ast::VariableDeclarationKind::Const
            )
        } else {
            false
        }
    })
}

/// Check if a statement is an expression statement with no side effects.
fn is_side_effect_free_statement(statement: &Statement<'_>) -> bool {
    let Statement::ExpressionStatement(expression_statement) = statement else {
        return false;
    };
    is_side_effect_free_expression(&expression_statement.expression)
}

/// Check if an expression is guaranteed to have no side effects.
fn is_side_effect_free_expression(expression: &Expression<'_>) -> bool {
    match expression {
        Expression::NumericLiteral(_)
        | Expression::BooleanLiteral(_)
        | Expression::NullLiteral(_) => true,
        Expression::StringLiteral(string) => {
            // Preserve directive prologues ("use strict", "use asm", etc.).
            !string.value.as_str().starts_with("use ")
        }
        // `void 0`, `void <literal>`, `typeof <identifier>` — no side effects.
        Expression::UnaryExpression(unary) => {
            matches!(
                unary.operator,
                oxc_syntax::operator::UnaryOperator::Void
                    | oxc_syntax::operator::UnaryOperator::Typeof
            ) && is_side_effect_free_expression(&unary.argument)
        }
        // `undefined`, `NaN`, `Infinity` — no side effects when standalone.
        Expression::Identifier(identifier) => {
            matches!(identifier.name.as_str(), "undefined" | "NaN" | "Infinity")
        }
        Expression::ParenthesizedExpression(paren) => {
            is_side_effect_free_expression(&paren.expression)
        }
        _ => false,
    }
}
