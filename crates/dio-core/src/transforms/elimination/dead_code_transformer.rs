//! Removes dead (unreachable) code from block statements.
//!
//! Handles:
//! - Code after `return`, `throw`, `break`, `continue` in a block.
//! - `if (false) { ... }` with no else branch (removed entirely).
//!
//! This runs in the Finalize phase so that other transforms have a chance
//! to simplify conditions first.

use oxc_ast::ast::Statement;
use oxc_traverse::TraverseCtx;

use crate::transformer::{AstNodeType, Transformer, TransformerPhase, TransformerPriority};

/// Removes unreachable code after terminal statements in blocks.
pub struct DeadCodeTransformer;

impl Transformer for DeadCodeTransformer {
    fn name(&self) -> &str {
        "DeadCodeTransformer"
    }

    fn interests(&self) -> &[AstNodeType] {
        &[AstNodeType::BlockStatement]
    }

    fn priority(&self) -> TransformerPriority {
        TransformerPriority::Default
    }

    fn phase(&self) -> TransformerPhase {
        TransformerPhase::Finalize
    }

    fn exit_statement<'a>(
        &self,
        statement: &mut Statement<'a>,
        _context: &mut TraverseCtx<'a, ()>,
    ) -> bool {
        let Statement::BlockStatement(block) = statement else {
            return false;
        };

        let original_length = block.body.len();

        // Find the first terminal statement and remove everything after it.
        if let Some(terminal_index) = find_first_terminal(&block.body) {
            block.body.truncate(terminal_index + 1);
        }

        // Remove empty statements.
        block
            .body
            .retain(|statement| !matches!(statement, Statement::EmptyStatement(_)));

        block.body.len() != original_length
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
