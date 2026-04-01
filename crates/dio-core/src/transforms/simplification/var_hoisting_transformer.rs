//! Hoists `var` declarations to the top of their scope block.
//!
//! JavaScript hoists `var` declarations to the top of their enclosing
//! function scope. This transformer makes that hoisting explicit by
//! moving uninitialized `var` declarations to the top of the statement
//! list. This enables the `DeclarationMergeTransformer` to merge
//! declarations with assignments that appear before them in the source.
//!
//! Only moves declarations without initializers (`var x;`). Declarations
//! with initializers (`var x = 1;`) stay in place since the initialization
//! happens at the original position.
//!
//! # Example
//!
//! ```js
//! // Before
//! x = 10;
//! f(x);
//! var x;
//!
//! // After
//! var x;
//! x = 10;
//! f(x);
//! ```

use oxc_allocator::Vec as ArenaVec;
use oxc_ast::ast::Statement;
use oxc_traverse::TraverseCtx;

use crate::transformer::{AstNodeType, Transformer, TransformerPhase, TransformerPriority};

/// Hoists uninitialized `var` declarations to the top of their scope.
pub struct VarHoistingTransformer;

impl Transformer for VarHoistingTransformer {
    fn name(&self) -> &str {
        "VarHoistingTransformer"
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
        _context: &mut TraverseCtx<'a, ()>,
    ) -> bool {
        // Find indices of uninitialized var declarations that are not already
        // at the top (before any non-var-declaration statement).
        let mut first_non_var_index: Option<usize> = None;
        let mut indices_to_hoist: Vec<usize> = Vec::new();

        for (index, statement) in statements.iter().enumerate() {
            if is_uninitialized_var_declaration(statement) {
                if let Some(first_non_var) = first_non_var_index
                    && index > first_non_var
                {
                    indices_to_hoist.push(index);
                }
            } else if first_non_var_index.is_none() {
                first_non_var_index = Some(index);
            }
        }

        if indices_to_hoist.is_empty() {
            return false;
        }

        // Remove declarations from their current positions and collect them.
        let mut hoisted: Vec<Statement<'a>> = Vec::new();
        for &index in indices_to_hoist.iter().rev() {
            let statement = statements.remove(index);
            hoisted.push(statement);
        }
        hoisted.reverse();

        // Insert at the insertion point (before the first non-var statement,
        // or at index 0 if all preceding statements are var declarations).
        let insert_at = first_non_var_index.unwrap_or(0);
        for (offset, statement) in hoisted.into_iter().enumerate() {
            statements.insert(insert_at + offset, statement);
        }

        true
    }
}

/// Check if a statement is a `var` declaration with no initializer on any declarator.
fn is_uninitialized_var_declaration(statement: &Statement<'_>) -> bool {
    let Statement::VariableDeclaration(declaration) = statement else {
        return false;
    };
    declaration.kind == oxc_ast::ast::VariableDeclarationKind::Var
        && declaration.declarations.iter().all(|d| d.init.is_none())
}
