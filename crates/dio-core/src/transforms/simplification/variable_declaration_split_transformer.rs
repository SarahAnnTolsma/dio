//! Splits multi-declarator variable declarations into individual statements.
//!
//! Obfuscated code often combines many variable declarations into a single
//! statement to reduce readability:
//!
//! ```js
//! // Before
//! var a = 1, b = 2, c = 3;
//!
//! // After
//! var a = 1;
//! var b = 2;
//! var c = 3;
//! ```
//!
//! This transformer normalizes these into one declaration per statement,
//! making the code easier to read and enabling other transformers (like
//! constant inlining) to remove individual declarations independently.

use oxc_allocator::Vec as ArenaVec;
use oxc_ast::ast::Statement;
use oxc_span::SPAN;
use oxc_traverse::TraverseCtx;

use crate::transformer::{AstNodeType, Transformer, TransformerPhase, TransformerPriority};

/// Splits `var a = 1, b = 2;` into `var a = 1; var b = 2;`.
pub struct VariableDeclarationSplitTransformer;

impl Transformer for VariableDeclarationSplitTransformer {
    fn name(&self) -> &str {
        "VariableDeclarationSplitTransformer"
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
        let mut changed = false;
        let mut index = 0;

        while index < statements.len() {
            let Statement::VariableDeclaration(declaration) = &statements[index] else {
                index += 1;
                continue;
            };

            if declaration.declarations.len() <= 1 {
                index += 1;
                continue;
            }

            let kind = declaration.kind;

            // Take the declarations out of the existing statement.
            // We need to replace the statement first, then build from the taken data.
            let old_statement =
                std::mem::replace(&mut statements[index], context.ast.statement_empty(SPAN));
            let Statement::VariableDeclaration(mut old_declaration) = old_statement else {
                unreachable!();
            };

            // Build individual declaration statements from each declarator.
            let declarators: Vec<_> = old_declaration.declarations.drain(..).collect();
            let declarator_count = declarators.len();

            // Replace the empty statement at `index` with the first declarator.
            let mut iter = declarators.into_iter();
            let first = iter.next().unwrap();
            let mut single_declarations = context.ast.vec_with_capacity(1);
            single_declarations.push(first);
            statements[index] = Statement::VariableDeclaration(
                context.ast.alloc(context.ast.variable_declaration(
                    SPAN,
                    kind,
                    single_declarations,
                    false,
                )),
            );

            // Insert remaining declarators after the first.
            let mut insert_position = index + 1;
            for declarator in iter {
                let mut single_declarations = context.ast.vec_with_capacity(1);
                single_declarations.push(declarator);
                let new_statement = Statement::VariableDeclaration(
                    context.ast.alloc(context.ast.variable_declaration(
                        SPAN,
                        kind,
                        single_declarations,
                        false,
                    )),
                );
                statements.insert(insert_position, new_statement);
                insert_position += 1;
            }

            index += declarator_count;
            changed = true;
        }

        changed
    }
}
