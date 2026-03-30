//! Removes dead (unreachable) code from statement lists.
//!
//! Handles:
//! - Code after `return`, `throw`, `break`, `continue` in any statement list.
//! - Empty statements.
//!
//! This runs in the Finalize phase so that other transforms have a chance
//! to simplify conditions first.
//!
//! Uses `enter_statements` (via `StatementList` interest) to operate on all
//! statement lists — block bodies, function bodies, program bodies, etc.

use oxc_allocator::Vec as ArenaVec;
use oxc_ast::ast::Statement;
use oxc_traverse::TraverseCtx;

use crate::operations;
use crate::transformer::{AstNodeType, Transformer, TransformerPhase, TransformerPriority};

/// Removes unreachable code after terminal statements in any statement list.
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

        // Find the first terminal statement and remove everything after it.
        if let Some(terminal_index) = find_first_terminal(statements) {
            for index in (terminal_index + 1..statements.len()).rev() {
                operations::remove_statement_at(statements, index, context);
            }
            statements.truncate(terminal_index + 1);
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
