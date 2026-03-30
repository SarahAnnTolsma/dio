//! Simplifies member access through global object aliases.
//!
//! Detects variables assigned to `window`, `self`, or `globalThis` that are
//! never reassigned, and replaces member access through the alias with direct
//! global references.
//!
//! # Examples
//!
//! ```js
//! // Before
//! var wn = window;
//! wn.Number("42");
//! wn.Math.ceil(1.5);
//!
//! // After
//! Number("42");
//! Math.ceil(1.5);
//! ```

use std::collections::HashSet;
use std::sync::Mutex;

use oxc_allocator::Vec as ArenaVec;
use oxc_ast::ast::{Expression, Statement};
use oxc_span::SPAN;
use oxc_syntax::symbol::SymbolId;
use oxc_traverse::TraverseCtx;

use crate::operations;
use crate::transformer::{AstNodeType, Transformer, TransformerPhase, TransformerPriority};

/// Known global object names whose properties are equivalent to bare globals.
const GLOBAL_OBJECT_NAMES: &[&str] = &["window", "self", "globalThis"];

/// Replaces member access through global object aliases with direct global references.
pub struct GlobalAliasSimplificationTransformer {
    /// Symbol IDs of variables assigned to a global object (`window`, `self`, `globalThis`).
    alias_symbols: Mutex<HashSet<SymbolId>>,
}

impl Default for GlobalAliasSimplificationTransformer {
    fn default() -> Self {
        Self {
            alias_symbols: Mutex::new(HashSet::new()),
        }
    }
}

impl Transformer for GlobalAliasSimplificationTransformer {
    fn name(&self) -> &str {
        "GlobalAliasSimplificationTransformer"
    }

    fn interests(&self) -> &[AstNodeType] {
        &[AstNodeType::StatementList, AstNodeType::MemberExpression]
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
        let mut alias_symbols = self.alias_symbols.lock().unwrap();

        // Scan for `var alias = window` / `var alias = self` / `var alias = globalThis`.
        for statement in statements.iter() {
            let Statement::VariableDeclaration(declaration) = statement else {
                continue;
            };

            for declarator in &declaration.declarations {
                let oxc_ast::ast::BindingPattern::BindingIdentifier(binding) = &declarator.id
                else {
                    continue;
                };

                let Some(symbol_id) = binding.symbol_id.get() else {
                    continue;
                };

                let Some(initializer) = &declarator.init else {
                    continue;
                };

                // Check if the initializer is a known global object identifier.
                let Expression::Identifier(identifier) = initializer else {
                    continue;
                };

                if !GLOBAL_OBJECT_NAMES.contains(&identifier.name.as_str()) {
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

                alias_symbols.insert(symbol_id);
            }
        }

        if alias_symbols.is_empty() {
            return false;
        }

        // Remove declarations whose only declarator is a global alias.
        let mut changed = false;
        for index in (0..statements.len()).rev() {
            let Statement::VariableDeclaration(declaration) = &statements[index] else {
                continue;
            };

            let all_alias = declaration.declarations.iter().all(|declarator| {
                if let oxc_ast::ast::BindingPattern::BindingIdentifier(binding) = &declarator.id
                    && let Some(symbol_id) = binding.symbol_id.get()
                {
                    return alias_symbols.contains(&symbol_id);
                }
                false
            });

            if all_alias {
                operations::remove_statement_at(statements, index, context);
                changed = true;
            }
        }

        changed
    }

    fn enter_expression<'a>(
        &self,
        expression: &mut Expression<'a>,
        context: &mut TraverseCtx<'a, ()>,
    ) -> bool {
        let alias_symbols = self.alias_symbols.lock().unwrap();
        if alias_symbols.is_empty() {
            return false;
        }

        // Match `alias.property` where alias is a tracked global alias.
        let Expression::StaticMemberExpression(member) = expression else {
            return false;
        };

        let Expression::Identifier(identifier) = &member.object else {
            return false;
        };

        // Resolve the identifier to a symbol and check if it's a tracked alias.
        let Some(reference_id) = identifier.reference_id.get() else {
            return false;
        };

        let reference = context.scoping().get_reference(reference_id);
        let Some(symbol_id) = reference.symbol_id() else {
            return false;
        };

        if !alias_symbols.contains(&symbol_id) {
            return false;
        }

        // Replace `alias.prop` with just `prop`.
        let property_name = member.property.name.as_str();
        let atom = context.ast.atom(property_name);
        let replacement = context.ast.expression_identifier(SPAN, atom);
        operations::replace_expression(expression, replacement, context);
        true
    }
}
