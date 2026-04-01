//! Annotates Browserify-bundled modules with named functions and JSDoc type
//! comments on require calls.
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
//! This transformer names anonymous module functions as `module_N`.
//! The `annotate_browserify_requires` post-processing function adds
//! `/** @type {module_N} */` comments to require calls.

use std::collections::HashMap;

use oxc_allocator::Vec as ArenaVec;
use oxc_ast::ast::{Expression, ObjectPropertyKind, PropertyKey, Statement};
use oxc_span::SPAN;
use oxc_traverse::TraverseCtx;

use crate::transformer::{AstNodeType, Transformer, TransformerPhase, TransformerPriority};
use crate::utils::unwrap_parens;

/// Annotates Browserify modules with named functions.
pub struct BrowserifyAnnotationTransformer;

impl Transformer for BrowserifyAnnotationTransformer {
    fn name(&self) -> &str {
        "BrowserifyAnnotationTransformer"
    }

    fn interests(&self) -> &[AstNodeType] {
        &[AstNodeType::StatementList]
    }

    fn priority(&self) -> TransformerPriority {
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
        let mut changed = false;

        // Phase 1: Find Browserify bundles and collect module info (immutable).
        let mut bundle_indices: Vec<usize> = Vec::new();
        for index in 0..statements.len() {
            if extract_browserify_modules(&statements[index]).is_some() {
                bundle_indices.push(index);
            }
        }

        if bundle_indices.is_empty() {
            return false;
        }

        // Phase 2: Name module functions (mutable).
        for index in bundle_indices {
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

                if function.id.is_none() {
                    let name = format!("module_{module_id}");
                    let atom = context.ast.atom(&name);
                    let binding = context.ast.binding_identifier(SPAN, atom);
                    function.id = Some(binding);
                    changed = true;
                }
            }
        }

        changed
    }
}

/// Post-process codegen output to add `/** @type {module_N} */` JSDoc comments
/// on require calls in Browserify bundles.
///
/// Parses the output to find the Browserify module structure, builds a
/// dependency map, then annotates `require("...")` calls with type comments.
pub fn annotate_browserify_requires(source: &str) -> String {
    // Parse the output to extract module dependency maps.
    // We look for patterns like:
    //   N: [function module_N(n, t, e) {
    // and dependency maps like:
    //   }, { "../common/Foo": 4, "../common/Bar": 5 }]
    let dep_map = build_dependency_map(source);
    if dep_map.is_empty() {
        return source.to_string();
    }

    // Build a reverse map: dependency name → module name (e.g., "../common/Foo" → "module_4").
    let reverse_map: HashMap<String, String> = dep_map
        .iter()
        .map(|(name, id)| (name.clone(), format!("module_{id}")))
        .collect();

    // Annotate require calls by placing `/** @type {module_N} */` before the
    // enclosing `var` declaration (so the type applies to the variable, not
    // the require function). If there's no `var` on the same line, place it
    // before the require call itself.
    let mut result = String::with_capacity(source.len() + 1024);
    let mut remaining = source;

    while !remaining.is_empty() {
        if let Some(pos) = find_require_call(remaining) {
            let call_start = pos;
            let after_paren = &remaining[call_start + 2..]; // skip `n(`
            let quote = after_paren.as_bytes()[0];
            if quote == b'"' || quote == b'\'' {
                if let Some(end_quote) = after_paren[1..].find(quote as char) {
                    let module_name = &after_paren[1..1 + end_quote];

                    if let Some(module_func_name) = reverse_map.get(module_name) {
                        let annotation = format!("/** @type {{{module_func_name}}} */ ");

                        // Look backwards on the same line for `var ` to attach
                        // the annotation to the variable declaration.
                        let line_before = &remaining[..call_start];
                        let line_start = line_before.rfind('\n').map_or(0, |p| p + 1);
                        let line_prefix = &remaining[line_start..call_start];

                        if let Some(var_offset) = line_prefix.rfind("var ") {
                            // Insert before `var`.
                            let insert_at = line_start + var_offset;
                            result.push_str(&remaining[..insert_at]);
                            result.push_str(&annotation);
                            remaining = &remaining[insert_at..];
                        } else {
                            // No var — insert before the require call.
                            result.push_str(&remaining[..call_start]);
                            result.push_str(&annotation);
                            remaining = &remaining[call_start..];
                        }

                        // Advance past the require call.
                        let close_paren = remaining.find(')').unwrap_or(0) + 1;
                        result.push_str(&remaining[..close_paren]);
                        remaining = &remaining[close_paren..];
                        continue;
                    }
                }
            }

            result.push_str(&remaining[..call_start + 1]);
            remaining = &remaining[call_start + 1..];
        } else {
            result.push_str(remaining);
            break;
        }
    }

    result
}

/// Find the position of the next `n("` or `n('` pattern that looks like a require call.
fn find_require_call(source: &str) -> Option<usize> {
    let bytes = source.as_bytes();
    for i in 0..bytes.len().saturating_sub(2) {
        if bytes[i] == b'n'
            && bytes[i + 1] == b'('
            && (bytes[i + 2] == b'"' || bytes[i + 2] == b'\'')
        {
            // Make sure `n` is not part of a larger identifier.
            if i > 0 {
                let prev = bytes[i - 1];
                if prev.is_ascii_alphanumeric() || prev == b'_' || prev == b'$' {
                    continue;
                }
            }
            return Some(i);
        }
    }
    None
}

/// Build a map of dependency name → module ID from the source.
fn build_dependency_map(source: &str) -> HashMap<String, i64> {
    let mut map = HashMap::new();

    // Find dependency objects: { "name": N, "name2": M }
    // These appear after `}, {` in the Browserify bundle.
    let mut search_from = 0;
    while let Some(pos) = source[search_from..].find("\":\u{20}") {
        let abs_pos = search_from + pos;

        // Walk backwards to find the opening quote of the key.
        let key_start = source[..abs_pos].rfind('"');
        if let Some(key_start) = key_start {
            let key = &source[key_start + 1..abs_pos];

            // Walk forward to find the numeric value after ": ".
            let after_colon = &source[abs_pos + 3..];
            let num_end = after_colon
                .find(|c: char| !c.is_ascii_digit())
                .unwrap_or(after_colon.len());
            if num_end > 0 {
                if let Ok(id) = after_colon[..num_end].parse::<i64>() {
                    // Only include paths that look like module dependencies.
                    if key.starts_with("./") || key.starts_with("../") {
                        map.insert(key.to_string(), id);
                    }
                }
            }
        }

        search_from = abs_pos + 3;
    }

    map
}

/// Extract the modules object from a Browserify IIFE statement (immutable).
fn extract_browserify_modules<'a>(
    statement: &'a Statement<'a>,
) -> Option<&'a oxc_ast::ast::ObjectExpression<'a>> {
    let Statement::ExpressionStatement(expression_statement) = statement else {
        return None;
    };
    let expression = unwrap_parens(&expression_statement.expression);

    let Expression::UnaryExpression(unary) = expression else {
        return None;
    };
    let Expression::CallExpression(call) = &unary.argument else {
        return None;
    };
    let Expression::FunctionExpression(function) = unwrap_parens(&call.callee) else {
        return None;
    };

    if function.params.items.len() != 3 {
        return None;
    }
    if call.arguments.len() != 3 {
        return None;
    }

    let Some(first_arg) = call.arguments[0].as_expression() else {
        return None;
    };
    let Expression::ObjectExpression(modules) = first_arg else {
        return None;
    };

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

/// Extract a numeric key from a property key.
fn extract_property_key_number(key: &PropertyKey<'_>) -> Option<i64> {
    match key {
        PropertyKey::NumericLiteral(number) => Some(number.value as i64),
        PropertyKey::StringLiteral(string) => string.value.parse::<i64>().ok(),
        _ => None,
    }
}
