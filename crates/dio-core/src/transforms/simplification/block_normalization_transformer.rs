//! Ensures control flow statements always use block statements for their bodies.
//!
//! - `if (x) foo();` -> `if (x) { foo(); }`
//! - `if (x) foo(); else bar();` -> `if (x) { foo(); } else { bar(); }`
//! - `while (x) foo();` -> `while (x) { foo(); }`
//! - `for (...) foo();` -> `for (...) { foo(); }`
//! - `for (x in y) foo();` -> `for (x in y) { foo(); }`
//! - `for (x of y) foo();` -> `for (x of y) { foo(); }`
//! - `do foo(); while (x);` -> `do { foo(); } while (x);`

use oxc_ast::ast::Statement;
use oxc_traverse::TraverseCtx;

use crate::operations;
use crate::transformer::{AstNodeType, Transformer, TransformerPhase, TransformerPriority};

/// Wraps bare statements in control flow bodies with block statements.
pub struct BlockNormalizationTransformer;

impl Transformer for BlockNormalizationTransformer {
    fn name(&self) -> &str {
        "BlockNormalizationTransformer"
    }

    fn interests(&self) -> &[AstNodeType] {
        &[
            AstNodeType::IfStatement,
            AstNodeType::ForStatement,
            AstNodeType::ForInStatement,
            AstNodeType::ForOfStatement,
            AstNodeType::WhileStatement,
            AstNodeType::DoWhileStatement,
        ]
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
        match statement {
            Statement::IfStatement(if_statement) => {
                let mut changed = wrap_in_block(&mut if_statement.consequent, context);
                if let Some(alternate) = &mut if_statement.alternate {
                    // Don't wrap `else if` — only wrap non-block, non-if alternates.
                    if !matches!(
                        alternate,
                        Statement::BlockStatement(_) | Statement::IfStatement(_)
                    ) {
                        changed |= wrap_in_block(alternate, context);
                    }
                }
                changed
            }
            Statement::ForStatement(for_statement) => {
                wrap_in_block(&mut for_statement.body, context)
            }
            Statement::ForInStatement(for_in_statement) => {
                wrap_in_block(&mut for_in_statement.body, context)
            }
            Statement::ForOfStatement(for_of_statement) => {
                wrap_in_block(&mut for_of_statement.body, context)
            }
            Statement::WhileStatement(while_statement) => {
                wrap_in_block(&mut while_statement.body, context)
            }
            Statement::DoWhileStatement(do_while_statement) => {
                wrap_in_block(&mut do_while_statement.body, context)
            }
            _ => false,
        }
    }
}

/// Wrap a statement in a block statement if it isn't one already.
/// Returns `true` if a wrapping was performed.
fn wrap_in_block<'a>(
    statement: &mut Statement<'a>,
    context: &mut TraverseCtx<'a, ()>,
) -> bool {
    if matches!(statement, Statement::BlockStatement(_)) {
        return false;
    }

    let original = std::mem::replace(
        statement,
        context.ast.statement_empty(oxc_span::SPAN),
    );
    let body = context.ast.vec_from_array([original]);
    *statement = operations::create_block_statement(body, context);
    true
}
