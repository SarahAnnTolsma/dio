//! Annotates Browserify-bundled modules with JSDoc comments and named functions.
//!
//! Browserify bundles look like:
//!
//! ```js
//! !function(t, e, i) { /* runtime */ }({
//!     1: [function(require, module, exports) { /* module body */ }, {"./dep": 2}],
//!     2: [function(require, module, exports) { /* module body */ }, {}],
//! }, {}, [9]);
//! ```
//!
//! This transformer:
//! - Adds `/** @module N — deps: ./foo (4), ./bar (5) */` comments before each module
//! - Names anonymous module functions as `module_N`

use std::collections::HashMap;

use oxc_allocator::Vec as ArenaVec;
use oxc_ast::ast::{Expression, ObjectPropertyKind, PropertyKey, Statement};
use oxc_span::SPAN;
use oxc_traverse::TraverseCtx;

use crate::transformer::{AstNodeType, Transformer, TransformerPhase, TransformerPriority};
use crate::utils::unwrap_parens;

/// Annotates Browserify modules with JSDoc comments and names.
pub struct BrowserifyAnnotationTransformer;

impl Transformer for BrowserifyAnnotationTransformer {
    fn name(&self) -> &str {
        "BrowserifyAnnotationTransformer"
    }

    fn interests(&self) -> &[AstNodeType] {
        &[AstNodeType::StatementList]
    }

    fn priority(&self) -> TransformerPriority {
        // Run last so other transformers have already cleaned up the code.
        TransformerPriority::Last
    }

    fn phase(&self) -> TransformerPhase {
        TransformerPhase::Finalize
    }

    fn enter_statements<'a>(
        &self,
        statements: &mut ArenaVec<'a, Statement<'a>>,
        context: &mut TraverseCtx<'a, ()>,
    ) -> bool {
        // Phase 1: Find Browserify bundles and collect module info (immutable).
        let mut bundle_indices: Vec<(usize, HashMap<i64, ModuleInfo>)> = Vec::new();
        for index in 0..statements.len() {
            if let Some(modules_object) = extract_browserify_modules(&statements[index]) {
                let module_info = extract_module_info(modules_object);
                if !module_info.is_empty() {
                    bundle_indices.push((index, module_info));
                }
            }
        }

        if bundle_indices.is_empty() {
            return false;
        }

        // Phase 2: Annotate module functions (mutable).
        // Navigate inline to avoid holding a reference across loop iterations.
        let mut changed = false;
        for (index, module_info) in bundle_indices {
            let Statement::ExpressionStatement(expression_statement) = &mut statements[index]
            else {
                continue;
            };
            let Expression::UnaryExpression(unary) = &mut expression_statement.expression else {
                continue;
            };
            let Expression::CallExpression(call) = &mut unary.argument else {
                continue;
            };
            let Some(first_arg) = call.arguments[0].as_expression_mut() else {
                continue;
            };
            let Expression::ObjectExpression(modules_object) = first_arg else {
                continue;
            };

            for property in modules_object.properties.iter_mut() {
                let ObjectPropertyKind::ObjectProperty(property) = property else {
                    continue;
                };

                let Some(module_id) = extract_property_key_number(&property.key) else {
                    continue;
                };

                let Expression::ArrayExpression(array) = &mut property.value else {
                    continue;
                };
                if array.elements.is_empty() {
                    continue;
                }

                let Some(first_element) = array.elements[0].as_expression_mut() else {
                    continue;
                };
                let Expression::FunctionExpression(function) = first_element else {
                    continue;
                };

                // Name the anonymous module function.
                if function.id.is_none() {
                    let name = format!("module_{module_id}");
                    let atom = context.ast.atom(&name);
                    let binding = context.ast.binding_identifier(SPAN, atom);
                    function.id = Some(binding);
                    changed = true;
                }

                // Add a directive-like annotation as the first statement.
                if let Some(info) = module_info.get(&module_id) {
                    if let Some(body) = &mut function.body {
                        // Skip if already annotated.
                        let already_annotated = body.statements.first().is_some_and(|stmt| {
                            if let Statement::ExpressionStatement(stmt) = stmt {
                                if let Expression::StringLiteral(lit) = &stmt.expression {
                                    return lit.value.starts_with("@module");
                                }
                            }
                            false
                        });

                        if !already_annotated && !body.statements.is_empty() {
                            let comment = build_module_comment(module_id, info);
                            let comment_atom = context.ast.atom(&comment);
                            let comment_literal =
                                context.ast.expression_string_literal(SPAN, comment_atom, None);
                            let comment_statement =
                                context.ast.statement_expression(SPAN, comment_literal);
                            body.statements.insert(0, comment_statement);
                            changed = true;
                        }
                    }
                }
            }
        }

        changed
    }
}

/// Information about a single module's dependencies.
struct ModuleInfo {
    /// Map of dependency name → module ID.
    dependencies: Vec<(String, i64)>,
}

/// Extract the modules object from a Browserify IIFE statement (immutable).
fn extract_browserify_modules<'a>(
    statement: &'a Statement<'a>,
) -> Option<&'a oxc_ast::ast::ObjectExpression<'a>> {
    let Statement::ExpressionStatement(expression_statement) = statement else {
        return None;
    };
    let expression = unwrap_parens(&expression_statement.expression);

    // Pattern: !function(t, e, i) { ... }({...}, {}, [...])
    let Expression::UnaryExpression(unary) = expression else {
        return None;
    };
    let Expression::CallExpression(call) = &unary.argument else {
        return None;
    };
    let Expression::FunctionExpression(function) = unwrap_parens(&call.callee) else {
        return None;
    };

    // The runtime function should have 3 parameters.
    if function.params.items.len() != 3 {
        return None;
    }

    // Must have 3 arguments: modules object, cache object, entries array.
    if call.arguments.len() != 3 {
        return None;
    }

    // First argument must be an object expression (the modules).
    let Some(first_arg) = call.arguments[0].as_expression() else {
        return None;
    };
    let Expression::ObjectExpression(modules) = first_arg else {
        return None;
    };

    // Verify it looks like Browserify modules: properties with numeric keys
    // whose values are arrays.
    let has_module_structure = modules.properties.iter().any(|prop| {
        if let ObjectPropertyKind::ObjectProperty(property) = prop {
            extract_property_key_number(&property.key).is_some()
                && matches!(&property.value, Expression::ArrayExpression(_))
        } else {
            false
        }
    });

    if !has_module_structure {
        return None;
    }

    Some(modules)
}

/// Extract module info (dependencies) from the modules object.
fn extract_module_info(
    modules: &oxc_ast::ast::ObjectExpression<'_>,
) -> HashMap<i64, ModuleInfo> {
    let mut info = HashMap::new();

    for property in &modules.properties {
        let ObjectPropertyKind::ObjectProperty(property) = property else {
            continue;
        };

        let Some(module_id) = extract_property_key_number(&property.key) else {
            continue;
        };

        let Expression::ArrayExpression(array) = &property.value else {
            continue;
        };

        // Second element is the dependency map: { "name": id, ... }
        let dependencies = if array.elements.len() >= 2 {
            if let Some(Expression::ObjectExpression(deps_object)) =
                array.elements[1].as_expression()
            {
                extract_dependency_map(deps_object)
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        info.insert(module_id, ModuleInfo { dependencies });
    }

    info
}

/// Extract dependency name → module ID pairs from a dependency object.
fn extract_dependency_map(
    deps: &oxc_ast::ast::ObjectExpression<'_>,
) -> Vec<(String, i64)> {
    let mut result = Vec::new();

    for property in &deps.properties {
        let ObjectPropertyKind::ObjectProperty(property) = property else {
            continue;
        };

        let name = match &property.key {
            PropertyKey::StringLiteral(string) => string.value.to_string(),
            PropertyKey::StaticIdentifier(identifier) => identifier.name.to_string(),
            _ => continue,
        };

        let Expression::NumericLiteral(number) = &property.value else {
            continue;
        };

        result.push((name, number.value as i64));
    }

    result
}

/// Extract a numeric key from a property key.
fn extract_property_key_number(key: &PropertyKey<'_>) -> Option<i64> {
    match key {
        PropertyKey::NumericLiteral(number) => Some(number.value as i64),
        PropertyKey::StringLiteral(string) => string.value.parse::<i64>().ok(),
        _ => None,
    }
}

/// Build a JSDoc-style comment string for a module.
fn build_module_comment(module_id: i64, info: &ModuleInfo) -> String {
    if info.dependencies.is_empty() {
        format!("@module {module_id}")
    } else {
        let deps: Vec<String> = info
            .dependencies
            .iter()
            .map(|(name, id)| format!("{name} ({id})"))
            .collect();
        format!("@module {module_id} — requires: {}", deps.join(", "))
    }
}
