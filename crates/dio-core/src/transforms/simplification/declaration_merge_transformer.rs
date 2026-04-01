//! Merges uninitialized variable declarations with their subsequent literal assignments.
//!
//! When a variable is declared without an initializer and then assigned a literal
//! value in the same block, this transformer combines them into a single initialized
//! declaration. This enables the ConstantInliningTransformer to inline the value.
//!
//! Only applies when the variable has exactly one write reference and the assignment
//! is a simple literal (string, numeric, boolean, null, undefined, or negated numeric).
//!
//! # Example
//!
//! ```js
//! // Before
//! var x;
//! x = -418;
//!
//! // After
//! var x = -418;
//! ```

use oxc_allocator::Vec as ArenaVec;
use oxc_ast::ast::{AssignmentTarget, Expression, Statement};
use oxc_span::SPAN;
use oxc_syntax::symbol::SymbolId;
use oxc_traverse::TraverseCtx;

use crate::operations;
use crate::transformer::{AstNodeType, Transformer, TransformerPhase, TransformerPriority};

/// Merges `var x; x = literal;` into `var x = literal;`.
pub struct DeclarationMergeTransformer;

impl Transformer for DeclarationMergeTransformer {
    fn name(&self) -> &str {
        "DeclarationMergeTransformer"
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
        // Collect merge candidates: (declaration index, assignment index, symbol_id).
        let mut merges: Vec<(usize, usize, SymbolId)> = Vec::new();

        for decl_index in 0..statements.len() {
            let Statement::VariableDeclaration(declaration) = &statements[decl_index] else {
                continue;
            };

            // Must be a single declarator with no initializer.
            if declaration.declarations.len() != 1 {
                continue;
            }
            let declarator = &declaration.declarations[0];
            if declarator.init.is_some() {
                continue;
            }

            let oxc_ast::ast::BindingPattern::BindingIdentifier(binding) = &declarator.id else {
                continue;
            };
            let Some(symbol_id) = binding.symbol_id.get() else {
                continue;
            };

            // Must have exactly one write reference.
            let reference_ids = context.scoping().get_resolved_reference_ids(symbol_id);
            let write_count = reference_ids
                .iter()
                .filter(|&&ref_id| context.scoping().get_reference(ref_id).is_write())
                .count();
            if write_count != 1 {
                continue;
            }

            // Must not be redeclared.
            let redeclarations = context.scoping().symbol_redeclarations(symbol_id);
            if !redeclarations.is_empty() {
                continue;
            }

            // Find the assignment statement in the same block, after the declaration.
            let binding_name = binding.name.to_string();
            for assign_index in (decl_index + 1)..statements.len() {
                let Statement::ExpressionStatement(expression_statement) =
                    &statements[assign_index]
                else {
                    continue;
                };
                let Expression::AssignmentExpression(assignment) = &expression_statement.expression
                else {
                    continue;
                };
                if assignment.operator != oxc_syntax::operator::AssignmentOperator::Assign {
                    continue;
                }

                // Left side must be the same variable.
                let AssignmentTarget::AssignmentTargetIdentifier(target) = &assignment.left else {
                    continue;
                };
                if target.name.as_str() != binding_name {
                    continue;
                }

                // Verify the reference resolves to the same symbol.
                let Some(ref_id) = target.reference_id.get() else {
                    continue;
                };
                let reference = context.scoping().get_reference(ref_id);
                if reference.symbol_id() != Some(symbol_id) {
                    continue;
                }

                // Right side must be a literal value.
                if !is_literal(&assignment.right) {
                    break; // Stop searching — the write is non-literal.
                }

                merges.push((decl_index, assign_index, symbol_id));
                break;
            }
        }

        if merges.is_empty() {
            return false;
        }

        // Apply merges in reverse order to preserve indices.
        for &(decl_index, assign_index, _symbol_id) in merges.iter().rev() {
            // Take the right-hand side from the assignment.
            let Statement::ExpressionStatement(expression_statement) =
                &mut statements[assign_index]
            else {
                continue;
            };
            let Expression::AssignmentExpression(assignment) = &mut expression_statement.expression
            else {
                continue;
            };
            let initializer = std::mem::replace(
                &mut assignment.right,
                context.ast.expression_null_literal(SPAN),
            );

            // Set it as the initializer on the declaration.
            let Statement::VariableDeclaration(declaration) = &mut statements[decl_index] else {
                continue;
            };
            declaration.declarations[0].init = Some(initializer);

            // Remove the assignment statement.
            operations::remove_statement_at(statements, assign_index, context);
        }

        true
    }
}

/// Check if an expression is a simple literal suitable for merging.
fn is_literal(expression: &Expression<'_>) -> bool {
    match expression {
        Expression::NumericLiteral(_)
        | Expression::StringLiteral(_)
        | Expression::BooleanLiteral(_)
        | Expression::NullLiteral(_) => true,
        // void 0 (undefined)
        Expression::UnaryExpression(unary) => {
            match unary.operator {
                oxc_syntax::operator::UnaryOperator::UnaryNegation => {
                    // -42
                    matches!(&unary.argument, Expression::NumericLiteral(_))
                }
                oxc_syntax::operator::UnaryOperator::Void => {
                    // void 0
                    matches!(&unary.argument, Expression::NumericLiteral(_))
                }
                _ => false,
            }
        }
        Expression::Identifier(identifier) => {
            matches!(identifier.name.as_str(), "undefined")
        }
        _ => false,
    }
}
