//! Converts variable declarations with anonymous function expression initializers
//! into function declarations.
//!
//! Only applies when the variable is never reassigned and the function expression
//! has no existing name.
//!
//! # Examples
//!
//! ```js
//! // Before
//! var foo = function() { return 1; };
//!
//! // After
//! function foo() { return 1; }
//! ```

use oxc_allocator::Vec as ArenaVec;
use oxc_ast::ast::{Expression, FunctionType, Statement};
use oxc_span::SPAN;
use oxc_traverse::TraverseCtx;

use crate::operations;
use crate::transformer::{AstNodeType, Transformer, TransformerPhase, TransformerPriority};

/// Converts `var x = function() { ... }` to `function x() { ... }`.
pub struct FunctionDeclarationTransformer;

impl Transformer for FunctionDeclarationTransformer {
    fn name(&self) -> &str {
        "FunctionDeclarationTransformer"
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

        for index in 0..statements.len() {
            let Statement::VariableDeclaration(declaration) = &statements[index] else {
                continue;
            };

            // Only handle single-declarator statements.
            if declaration.declarations.len() != 1 {
                continue;
            }

            let declarator = &declaration.declarations[0];

            // Must have a simple binding identifier.
            let oxc_ast::ast::BindingPattern::BindingIdentifier(binding) = &declarator.id else {
                continue;
            };

            let Some(symbol_id) = binding.symbol_id.get() else {
                continue;
            };

            // Must be initialized with an anonymous function expression.
            let Some(initializer) = &declarator.init else {
                continue;
            };
            let Expression::FunctionExpression(function) = initializer else {
                continue;
            };

            // Skip named function expressions (e.g., `var x = function y() {}`).
            if function.id.is_some() {
                continue;
            }

            // Must not be reassigned.
            let reference_ids = context.scoping().get_resolved_reference_ids(symbol_id);
            let has_writes = reference_ids.iter().any(|&reference_id| {
                context.scoping().get_reference(reference_id).is_write()
            });
            if has_writes {
                continue;
            }

            // Must not be redeclared.
            let redeclarations = context.scoping().symbol_redeclarations(symbol_id);
            if !redeclarations.is_empty() {
                continue;
            }

            // Copy the binding name before taking a mutable borrow.
            let binding_name = binding.name.to_string();

            // Build the replacement: take the function expression out and convert it
            // to a function declaration with the variable's binding identifier.
            let Statement::VariableDeclaration(declaration) = &mut statements[index] else {
                continue;
            };
            let declarator = &mut declaration.declarations[0];

            // Take the function expression out of the initializer.
            let initializer = std::mem::replace(
                &mut declarator.init,
                Some(context.ast.expression_null_literal(SPAN)),
            );
            let Some(Expression::FunctionExpression(mut function)) = initializer else {
                continue;
            };

            // Create a binding identifier for the function name from the variable name.
            let name = context.ast.atom(&binding_name);
            let function_binding =
                context
                    .ast
                    .binding_identifier_with_symbol_id(SPAN, name, symbol_id);

            // Convert the function expression to a function declaration.
            function.r#type = FunctionType::FunctionDeclaration;
            function.id = Some(function_binding);

            let replacement = Statement::FunctionDeclaration(function);
            operations::replace_statement(&mut statements[index], replacement, context);
            changed = true;
        }

        changed
    }
}
