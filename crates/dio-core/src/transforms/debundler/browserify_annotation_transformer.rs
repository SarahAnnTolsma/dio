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

        // Phase 1: Find Browserify bundles and build module name map (immutable).
        let mut bundle_indices: Vec<(usize, HashMap<i64, String>)> = Vec::new();
        for index in 0..statements.len() {
            if let Some(modules_object) = extract_browserify_modules(&statements[index]) {
                let info = extract_module_info(modules_object);
                let name_map = build_module_name_map(&info);
                bundle_indices.push((index, name_map));
            }
        }

        if bundle_indices.is_empty() {
            return false;
        }

        // Phase 2: Name module functions (mutable).
        for (index, name_map) in &bundle_indices {
            let index = *index;
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
                    let name = name_map
                        .get(&module_id)
                        .cloned()
                        .unwrap_or_else(|| format!("module_{module_id}"));
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

    // Build a reverse map: dependency name → readable module name.
    let reverse_map: HashMap<String, String> = dep_map
        .keys()
        .map(|name| (name.clone(), path_to_identifier(name)))
        .collect();

    // Extract the set of require parameter names from module function signatures.
    // Pattern: `function module_name(PARAM, ...` — PARAM is the require function.
    let require_names = extract_require_parameter_names(source);
    if require_names.is_empty() {
        return source.to_string();
    }

    // Annotate require calls by placing `/** @type {name} */` before the
    // enclosing `var` declaration (so the type applies to the variable, not
    // the require function). If there's no `var` on the same line, place it
    // before the require call itself.
    let mut result = String::with_capacity(source.len() + 1024);
    let mut remaining = source;

    while !remaining.is_empty() {
        if let Some((pos, param_len)) = find_require_call(remaining, &require_names) {
            let call_start = pos;
            let after_paren = &remaining[call_start + param_len + 1..]; // skip `name(`
            let quote = after_paren.as_bytes()[0];
            if (quote == b'"' || quote == b'\'')
                && let Some(end_quote) = after_paren[1..].find(quote as char)
            {
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

            result.push_str(&remaining[..call_start + 1]);
            remaining = &remaining[call_start + 1..];
        } else {
            result.push_str(remaining);
            break;
        }
    }

    result
}

/// Find the position of the next `name("` or `name('` pattern that looks like
/// a require call, where `name` is one of the known require parameter names.
/// Returns the position and the length of the parameter name.
fn find_require_call(source: &str, require_names: &[String]) -> Option<(usize, usize)> {
    let bytes = source.as_bytes();
    for i in 0..bytes.len() {
        // Check if this position is the start of an identifier (not mid-identifier).
        if i > 0 {
            let prev = bytes[i - 1];
            if prev.is_ascii_alphanumeric() || prev == b'_' || prev == b'$' {
                continue;
            }
        }

        for name in require_names {
            let name_bytes = name.as_bytes();
            let end = i + name_bytes.len();
            if end + 2 > bytes.len() {
                continue;
            }
            if &bytes[i..end] == name_bytes
                && bytes[end] == b'('
                && (bytes[end + 1] == b'"' || bytes[end + 1] == b'\'')
            {
                // Verify the name isn't part of a larger identifier.
                if end < bytes.len() && bytes[end] == b'(' {
                    return Some((i, name_bytes.len()));
                }
            }
        }
    }
    None
}

/// Extract require parameter names from Browserify module function signatures.
///
/// Looks for `function name(PARAM,` patterns in the source and collects
/// unique first parameter names.
fn extract_require_parameter_names(source: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();

    // Match: `[function name(PARAM,` — the first param after the opening paren.
    let mut search_from = 0;
    while let Some(pos) = source[search_from..].find("[function ") {
        let abs_pos = search_from + pos;
        let after_func = &source[abs_pos + 10..]; // skip `[function `

        // Find the opening paren.
        if let Some(paren_pos) = after_func.find('(') {
            let after_paren = &after_func[paren_pos + 1..];
            // Extract the first parameter name (up to comma or close paren).
            let param_end = after_paren.find([',', ')', ' ']).unwrap_or(0);
            if param_end > 0 {
                let param_name = &after_paren[..param_end];
                if !param_name.is_empty()
                    && param_name
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
                    && !names.contains(&param_name.to_string())
                {
                    names.push(param_name.to_string());
                }
            }
        }

        search_from = abs_pos + 10;
    }

    names
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
            if num_end > 0
                && let Ok(id) = after_colon[..num_end].parse::<i64>()
            {
                // Only include paths that look like module dependencies.
                if key.starts_with("./") || key.starts_with("../") {
                    map.insert(key.to_string(), id);
                }
            }
        }

        search_from = abs_pos + 3;
    }

    map
}

/// Information about a single module's dependencies.
struct ModuleInfo {
    dependencies: Vec<(String, i64)>,
}

/// Extract module info (dependencies) from the modules object.
fn extract_module_info(modules: &oxc_ast::ast::ObjectExpression<'_>) -> HashMap<i64, ModuleInfo> {
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
fn extract_dependency_map(deps: &oxc_ast::ast::ObjectExpression<'_>) -> Vec<(String, i64)> {
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

/// Build a map of module ID → readable function name from dependency info.
///
/// Derives names from dependency paths: `../common/DataDomeTools` → `common_DataDomeTools`.
/// Modules with no incoming dependency references get `module_N` as a fallback.
fn build_module_name_map(info: &HashMap<i64, ModuleInfo>) -> HashMap<i64, String> {
    // Collect all (target_id, path) pairs from dependency maps.
    let mut id_to_paths: HashMap<i64, Vec<&str>> = HashMap::new();
    for module_info in info.values() {
        for (path, target_id) in &module_info.dependencies {
            id_to_paths.entry(*target_id).or_default().push(path);
        }
    }

    let mut name_map = HashMap::new();
    for &module_id in info.keys() {
        let name = if let Some(paths) = id_to_paths.get(&module_id) {
            // Pick the shortest path as the canonical name.
            let best_path = paths.iter().min_by_key(|p| p.len()).unwrap();
            path_to_identifier(best_path)
        } else {
            format!("module_{module_id}")
        };
        name_map.insert(module_id, name);
    }

    name_map
}

/// Convert a module path like `../common/DataDomeTools.js` to a valid
/// JS identifier like `common_DataDomeTools`.
fn path_to_identifier(path: &str) -> String {
    // Strip leading ./ and ../
    let mut cleaned = path;
    while cleaned.starts_with("../") {
        cleaned = &cleaned[3..];
    }
    if cleaned.starts_with("./") {
        cleaned = &cleaned[2..];
    }

    // Strip .js extension.
    if cleaned.ends_with(".js") {
        cleaned = &cleaned[..cleaned.len() - 3];
    }

    // Replace path separators and invalid chars with underscores.
    let result: String = cleaned
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '$' {
                c
            } else {
                '_'
            }
        })
        .collect();

    // Ensure it starts with a valid identifier character.
    if result.is_empty() || result.starts_with(|c: char| c.is_ascii_digit()) {
        format!("module_{result}")
    } else {
        result
    }
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

    let Expression::ObjectExpression(modules) = call.arguments[0].as_expression()? else {
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
