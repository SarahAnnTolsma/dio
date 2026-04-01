//! Inlines constants that are assigned once and never reassigned.
//!
//! Identifies variable declarations (`var`, `let`, or `const`) where the binding
//! has a literal initializer and zero write references (meaning it is never
//! reassigned). All read references to such bindings are replaced with a copy of
//! the literal value, and the now-dead declaration is removed.
//!
//! This is particularly important for obfuscated code which typically uses `var`
//! rather than `const`, so we cannot rely on the declaration keyword alone — we
//! must check whether the variable has any write references via scoping.
//!
//! # Examples
//!
//! ```js
//! // Before
//! var x = 5;
//! console.log(x);
//!
//! // After
//! console.log(5);
//! ```
//!
//! ```js
//! // Before
//! var greeting = "hello";
//! var x = greeting;
//!
//! // After
//! var x = "hello";
//! ```

use std::collections::HashMap;
use std::sync::Mutex;

use oxc_allocator::Vec as ArenaVec;
use oxc_ast::ast::{Expression, Statement};
use oxc_span::SPAN;
use oxc_syntax::number::NumberBase;
use oxc_syntax::symbol::SymbolId;
use oxc_traverse::TraverseCtx;

use crate::operations;
use crate::transformer::{AstNodeType, Transformer, TransformerPhase, TransformerPriority};

/// A constant literal value that can be inlined at reference sites.
#[derive(Debug, Clone)]
enum ConstantValue {
    Number(f64),
    String(String),
    Boolean(bool),
    Null,
    Undefined,
}

/// Inlines constant variables that are assigned once to a literal value.
///
/// Uses `enter_statements` to scan declarations and build a map of inlinable
/// constants, then uses `enter_expression` to replace identifier references
/// with the literal values.
pub struct ConstantInliningTransformer {
    /// Maps symbol IDs to their constant literal values. Populated during
    /// `enter_statements` and consumed during `enter_expression`.
    constants: Mutex<HashMap<SymbolId, ConstantValue>>,

    /// Symbol IDs whose declarations should be removed because all references
    /// have been inlined.
    symbols_to_remove: Mutex<Vec<SymbolId>>,
}

impl ConstantInliningTransformer {
    /// Scan a variable declaration for inlinable constants.
    fn scan_declaration(
        declaration: &oxc_ast::ast::VariableDeclaration<'_>,
        constants: &mut HashMap<SymbolId, ConstantValue>,
        context: &TraverseCtx<'_, ()>,
    ) {
        for declarator in &declaration.declarations {
            let oxc_ast::ast::BindingPattern::BindingIdentifier(binding) = &declarator.id else {
                continue;
            };

            let Some(symbol_id) = binding.symbol_id.get() else {
                continue;
            };

            let Some(init) = &declarator.init else {
                continue;
            };

            let Some(value) = Self::extract_constant_value(init) else {
                continue;
            };

            if Self::has_write_references(symbol_id, context) {
                continue;
            }

            let redeclarations = context.scoping().symbol_redeclarations(symbol_id);
            if !redeclarations.is_empty() {
                continue;
            }

            let reference_ids = context.scoping().get_resolved_reference_ids(symbol_id);
            if reference_ids.is_empty() {
                continue;
            }

            constants.insert(symbol_id, value);
        }
    }

    /// Check whether a symbol has any write references (assignments, updates, etc.).
    fn has_write_references(symbol_id: SymbolId, context: &TraverseCtx<'_, ()>) -> bool {
        let reference_ids = context.scoping().get_resolved_reference_ids(symbol_id);
        reference_ids
            .iter()
            .any(|&reference_id| context.scoping().get_reference(reference_id).is_write())
    }

    /// Extract a `ConstantValue` from a literal expression, if possible.
    fn extract_constant_value(expression: &Expression<'_>) -> Option<ConstantValue> {
        match expression {
            Expression::NumericLiteral(literal) => Some(ConstantValue::Number(literal.value)),
            Expression::StringLiteral(literal) => {
                Some(ConstantValue::String(literal.value.to_string()))
            }
            Expression::BooleanLiteral(literal) => Some(ConstantValue::Boolean(literal.value)),
            Expression::NullLiteral(_) => Some(ConstantValue::Null),
            // void 0 → undefined
            Expression::UnaryExpression(unary)
                if unary.operator == oxc_syntax::operator::UnaryOperator::Void =>
            {
                if let Expression::NumericLiteral(literal) = &unary.argument
                    && literal.value == 0.0
                {
                    return Some(ConstantValue::Undefined);
                }
                None
            }
            _ => None,
        }
    }

    /// Build a replacement expression from a constant value.
    fn build_replacement<'a>(
        value: &ConstantValue,
        context: &mut TraverseCtx<'a, ()>,
    ) -> Expression<'a> {
        match value {
            ConstantValue::Number(number) => {
                let raw = context.ast.atom(&number.to_string());
                if *number < 0.0 {
                    // Negative numbers need to be represented as unary negation
                    let positive_raw = context.ast.atom(&(-number).to_string());
                    let positive = context.ast.expression_numeric_literal(
                        SPAN,
                        -number,
                        Some(positive_raw),
                        NumberBase::Decimal,
                    );
                    context.ast.expression_unary(
                        SPAN,
                        oxc_syntax::operator::UnaryOperator::UnaryNegation,
                        positive,
                    )
                } else {
                    context.ast.expression_numeric_literal(
                        SPAN,
                        *number,
                        Some(raw),
                        NumberBase::Decimal,
                    )
                }
            }
            ConstantValue::String(string) => {
                let value = context.ast.atom(string);
                context.ast.expression_string_literal(SPAN, value, None)
            }
            ConstantValue::Boolean(value) => context.ast.expression_boolean_literal(SPAN, *value),
            ConstantValue::Null => context.ast.expression_null_literal(SPAN),
            ConstantValue::Undefined => context.ast.void_0(SPAN),
        }
    }
}

impl Default for ConstantInliningTransformer {
    fn default() -> Self {
        Self {
            constants: Mutex::new(HashMap::new()),
            symbols_to_remove: Mutex::new(Vec::new()),
        }
    }
}

impl Transformer for ConstantInliningTransformer {
    fn name(&self) -> &str {
        "ConstantInliningTransformer"
    }

    fn interests(&self) -> &[AstNodeType] {
        &[AstNodeType::StatementList, AstNodeType::Identifier]
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
        let mut constants = self.constants.lock().unwrap();
        let mut symbols_to_remove = self.symbols_to_remove.lock().unwrap();

        // Phase 1: Scan for inlinable constants in variable declarations,
        // including those in for-statement inits (var in for-inits are scoped
        // to the enclosing function/block, not the loop).
        for statement in statements.iter() {
            match statement {
                Statement::VariableDeclaration(declaration) => {
                    Self::scan_declaration(declaration, &mut constants, context);
                }
                Statement::ForStatement(for_statement) => {
                    if let Some(oxc_ast::ast::ForStatementInit::VariableDeclaration(declaration)) =
                        &for_statement.init
                    {
                        Self::scan_declaration(declaration, &mut constants, context);
                    }
                }
                Statement::ForInStatement(for_in) => {
                    if let oxc_ast::ast::ForStatementLeft::VariableDeclaration(declaration) =
                        &for_in.left
                    {
                        Self::scan_declaration(declaration, &mut constants, context);
                    }
                }
                Statement::ForOfStatement(for_of) => {
                    if let oxc_ast::ast::ForStatementLeft::VariableDeclaration(declaration) =
                        &for_of.left
                    {
                        Self::scan_declaration(declaration, &mut constants, context);
                    }
                }
                _ => {}
            }
        }

        if constants.is_empty() {
            return false;
        }

        // Phase 2: Remove declarations for constants that will be inlined.
        let mut changed = false;
        for index in (0..statements.len()).rev() {
            match &statements[index] {
                Statement::VariableDeclaration(declaration) => {
                    let all_inlinable = declaration.declarations.iter().all(|declarator| {
                        if let oxc_ast::ast::BindingPattern::BindingIdentifier(binding) =
                            &declarator.id
                            && let Some(symbol_id) = binding.symbol_id.get()
                        {
                            return constants.contains_key(&symbol_id);
                        }
                        false
                    });

                    if all_inlinable {
                        for declarator in &declaration.declarations {
                            if let oxc_ast::ast::BindingPattern::BindingIdentifier(binding) =
                                &declarator.id
                                && let Some(symbol_id) = binding.symbol_id.get()
                                && constants.contains_key(&symbol_id)
                            {
                                symbols_to_remove.push(symbol_id);
                            }
                        }
                        operations::remove_statement_at(statements, index, context);
                        changed = true;
                    }
                }
                Statement::ForStatement(_)
                | Statement::ForInStatement(_)
                | Statement::ForOfStatement(_) => {
                    // For for-init declarations, null out the initializers of
                    // inlined constants. The unused variable transformer will
                    // clean up the empty declarators later.
                    let declaration = match &mut statements[index] {
                        Statement::ForStatement(f) => {
                            if let Some(oxc_ast::ast::ForStatementInit::VariableDeclaration(d)) =
                                &mut f.init
                            {
                                Some(&mut **d)
                            } else {
                                None
                            }
                        }
                        _ => None,
                    };

                    if let Some(declaration) = declaration {
                        for declarator in declaration.declarations.iter_mut() {
                            if let oxc_ast::ast::BindingPattern::BindingIdentifier(binding) =
                                &declarator.id
                                && let Some(symbol_id) = binding.symbol_id.get()
                                && constants.contains_key(&symbol_id)
                                && declarator.init.is_some()
                            {
                                declarator.init = None;
                                symbols_to_remove.push(symbol_id);
                                changed = true;
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        changed
    }

    fn enter_expression<'a>(
        &self,
        expression: &mut Expression<'a>,
        context: &mut TraverseCtx<'a, ()>,
    ) -> bool {
        let Expression::Identifier(identifier) = expression else {
            return false;
        };

        let Some(reference_id) = identifier.reference_id.get() else {
            return false;
        };

        let reference = context.scoping().get_reference(reference_id);
        let Some(symbol_id) = reference.symbol_id() else {
            return false;
        };

        let constants = self.constants.lock().unwrap();
        let Some(value) = constants.get(&symbol_id) else {
            return false;
        };

        let replacement = Self::build_replacement(value, context);
        operations::replace_expression(expression, replacement, context);
        true
    }
}
