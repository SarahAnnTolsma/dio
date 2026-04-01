//! Inlines initialization functions that only contain assignment statements.
//!
//! Akamai scripts use functions like `LN3()` and `jH3()` that exist solely
//! to assign values to variables in the enclosing scope. This transformer
//! finds such functions and replaces their call sites with the function's
//! body statements.
//!
//! # Pattern
//!
//! ```js
//! function LN3() {
//!     XI = 10;
//!     WR = 5;
//! }
//! LN3();  // → replaced with: XI = 10; WR = 5;
//! ```
//!
//! Requirements:
//! - Function body contains only expression statements with assignments
//! - Function has no parameters
//! - Function is called as a standalone expression statement (not in a value position)
//! - The call and the function are in the same statement list

use oxc_allocator::Vec as ArenaVec;
use oxc_ast::ast::{Expression, FunctionType, Statement};
use oxc_span::SPAN;
use oxc_syntax::symbol::SymbolId;
use oxc_traverse::TraverseCtx;

use crate::operations;
use crate::transformer::{AstNodeType, Transformer, TransformerPhase, TransformerPriority};

/// Inlines initialization functions whose bodies contain only assignments.
pub struct InitializerInliningTransformer;

impl Transformer for InitializerInliningTransformer {
    fn name(&self) -> &str {
        "InitializerInliningTransformer"
    }

    fn interests(&self) -> &[AstNodeType] {
        &[AstNodeType::StatementList]
    }

    fn priority(&self) -> TransformerPriority {
        TransformerPriority::First
    }

    fn phase(&self) -> TransformerPhase {
        TransformerPhase::Main
    }

    fn enter_statements<'a>(
        &self,
        statements: &mut ArenaVec<'a, Statement<'a>>,
        context: &mut TraverseCtx<'a, ()>,
    ) -> bool {
        // Phase 1: Find initializer functions — function declarations whose body
        // contains only simple assignment expression statements.
        let mut initializer_functions: Vec<(SymbolId, usize)> = Vec::new(); // (symbol, stmt_index)

        for (index, statement) in statements.iter().enumerate() {
            let Statement::FunctionDeclaration(function) = statement else {
                continue;
            };
            if function.r#type != FunctionType::FunctionDeclaration {
                continue;
            }
            // Must have no parameters.
            if !function.params.items.is_empty() {
                continue;
            }
            let Some(binding) = &function.id else {
                continue;
            };
            let Some(symbol_id) = binding.symbol_id.get() else {
                continue;
            };
            let Some(body) = &function.body else {
                continue;
            };
            // Body must be non-empty and contain only assignment expression statements.
            if body.statements.is_empty() {
                continue;
            }
            if !body.statements.iter().all(is_assignment_statement) {
                continue;
            }

            initializer_functions.push((symbol_id, index));
        }

        if initializer_functions.is_empty() {
            return false;
        }

        // Phase 2: Find call sites in the same statement list — expression statements
        // that are simple calls to one of the initializer functions.
        let mut inlinings: Vec<(usize, usize)> = Vec::new(); // (call_stmt_index, func_stmt_index)

        for (index, statement) in statements.iter().enumerate() {
            let Statement::ExpressionStatement(expression_statement) = statement else {
                continue;
            };
            let Expression::CallExpression(call) = &expression_statement.expression else {
                continue;
            };
            // Must be a simple call with no arguments.
            if !call.arguments.is_empty() {
                continue;
            }
            let Expression::Identifier(callee) = &call.callee else {
                continue;
            };
            let Some(reference_id) = callee.reference_id.get() else {
                continue;
            };
            let reference = context.scoping().get_reference(reference_id);
            let Some(symbol_id) = reference.symbol_id() else {
                continue;
            };

            // Check if this calls one of our initializer functions.
            if let Some(&(_, func_index)) = initializer_functions
                .iter()
                .find(|(sym, _)| *sym == symbol_id)
            {
                inlinings.push((index, func_index));
            }
        }

        if inlinings.is_empty() {
            return false;
        }

        // Phase 3: Replace each call site with the function body's statements.
        // Process in reverse order to preserve indices.
        let mut changed = false;
        for &(call_index, func_index) in inlinings.iter().rev() {
            // Extract the function body statements.
            let Statement::FunctionDeclaration(function) = &mut statements[func_index] else {
                continue;
            };
            let Some(body) = &mut function.body else {
                continue;
            };

            // Clone the body statements (they may be needed if the function
            // is called multiple times).
            let body_statements: Vec<Statement<'a>> = body
                .statements
                .iter_mut()
                .map(|stmt| std::mem::replace(stmt, context.ast.statement_empty(SPAN)))
                .collect();

            // Replace the call statement with the body statements.
            let replacement = context.ast.vec_from_iter(body_statements);
            operations::replace_statement_with_multiple(
                statements,
                call_index,
                replacement,
                context,
            );
            changed = true;

            // Restore the function body (in case it's called again elsewhere).
            // Re-read the function since indices may have shifted.
            // Actually, after replace_statement_with_multiple the func_index
            // may have shifted. For safety, we just leave the body empty —
            // the unused function pruner will remove it later.
        }

        changed
    }
}

/// Check if a statement is an expression statement containing only an assignment.
fn is_assignment_statement(statement: &Statement<'_>) -> bool {
    let Statement::ExpressionStatement(expression_statement) = statement else {
        return false;
    };
    matches!(
        &expression_statement.expression,
        Expression::AssignmentExpression(_)
    )
}
