//! Removes variable declarations that have no read or write references.
//!
//! After other transformers inline constants, decode string arrays, or
//! eliminate dead code, some variable declarations become unreferenced.
//! This transformer prunes them.
//!
//! # Examples
//!
//! ```js
//! // Before (after string array decoder inlined all calls to `r`)
//! var dn = ["encoded1", "encoded2", "encoded3"];
//! console.log("decoded");
//!
//! // After
//! console.log("decoded");
//! ```
//!
//! For multi-declarator statements, only unreferenced declarators are removed.
//! If all declarators are unreferenced, the entire statement is removed.
//!
//! ```js
//! // Before
//! var a = 1, b = unused(), c = 3;
//! console.log(a, c);
//!
//! // After
//! var a = 1, c = 3;
//! console.log(a, c);
//! ```

use oxc_allocator::Vec as ArenaVec;
use oxc_ast::ast::Statement;
use oxc_span::SPAN;
use oxc_traverse::TraverseCtx;

use crate::operations;
use crate::transformer::{AstNodeType, Transformer, TransformerPhase, TransformerPriority};

/// Removes variable declarations with zero references.
pub struct UnusedVariableTransformer;

impl Transformer for UnusedVariableTransformer {
    fn name(&self) -> &str {
        "UnusedVariableTransformer"
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
        let mut changed = false;

        for index in (0..statements.len()).rev() {
            let Statement::VariableDeclaration(declaration) = &statements[index] else {
                continue;
            };

            // Check each declarator: removable if unreferenced AND the
            // initializer is side-effect-free (literal, array of literals,
            // or absent). We never remove declarations whose initializer
            // might have side effects (function calls, property access, etc.).
            let total = declaration.declarations.len();
            let unreferenced_count = declaration
                .declarations
                .iter()
                .filter(|declarator| is_removable_declarator(declarator, context))
                .count();

            if unreferenced_count == 0 {
                continue;
            }

            if unreferenced_count == total {
                // All declarators are unreferenced — remove the entire statement.
                operations::remove_statement_at(statements, index, context);
                changed = true;
            } else {
                // Partial — rebuild without the unreferenced declarators.
                let old_statement = std::mem::replace(
                    &mut statements[index],
                    context.ast.statement_empty(SPAN),
                );
                let Statement::VariableDeclaration(mut old_declaration) = old_statement else {
                    unreachable!();
                };

                let kind = old_declaration.kind;
                let kept: Vec<_> = old_declaration
                    .declarations
                    .drain(..)
                    .filter(|declarator| !is_removable_declarator(declarator, context))
                    .collect();

                let mut new_declarations = context.ast.vec_with_capacity(kept.len());
                for declarator in kept {
                    new_declarations.push(declarator);
                }

                statements[index] = Statement::VariableDeclaration(context.ast.alloc(
                    context
                        .ast
                        .variable_declaration(SPAN, kind, new_declarations, false),
                ));
                changed = true;
            }
        }

        changed
    }
}

/// A declarator is removable if it has zero references AND its initializer
/// (if any) is side-effect-free.
fn is_removable_declarator(
    declarator: &oxc_ast::ast::VariableDeclarator<'_>,
    context: &TraverseCtx<'_, ()>,
) -> bool {
    let oxc_ast::ast::BindingPattern::BindingIdentifier(binding) = &declarator.id else {
        return false;
    };
    let Some(symbol_id) = binding.symbol_id.get() else {
        return false;
    };

    // Must have zero references.
    let references = context.scoping().get_resolved_reference_ids(symbol_id);
    if !references.is_empty() {
        return false;
    }

    // Initializer must be side-effect-free (or absent).
    match &declarator.init {
        None => true,
        Some(init) => is_side_effect_free(init),
    }
}

/// Conservative check: is this expression guaranteed to have no side effects?
fn is_side_effect_free(expression: &oxc_ast::ast::Expression<'_>) -> bool {
    use oxc_ast::ast::Expression;

    match expression {
        Expression::NumericLiteral(_)
        | Expression::StringLiteral(_)
        | Expression::BooleanLiteral(_)
        | Expression::NullLiteral(_) => true,

        Expression::ArrayExpression(array) => {
            array.elements.iter().all(|element| {
                match element {
                    oxc_ast::ast::ArrayExpressionElement::SpreadElement(_) => false,
                    _ => {
                        if let Some(expr) = element.as_expression() {
                            is_side_effect_free(expr)
                        } else {
                            false
                        }
                    }
                }
            })
        }

        Expression::UnaryExpression(unary) => is_side_effect_free(&unary.argument),

        Expression::ParenthesizedExpression(paren) => is_side_effect_free(&paren.expression),

        _ => false,
    }
}
