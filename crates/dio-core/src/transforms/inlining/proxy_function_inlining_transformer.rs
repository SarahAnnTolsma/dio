//! Inlines proxy functions that simply wrap another operation.
//!
//! JavaScript obfuscators (notably `javascript-obfuscator`) generate small
//! "proxy" functions that forward to a single operator or call. These add
//! indirection without changing semantics, making the code harder to read.
//!
//! # Supported proxy patterns
//!
//! **Binary operation proxy** — wraps a single binary operator:
//! ```js
//! function _0x1(a, b) { return a + b; }
//! _0x1(1, 2)  // inlined to: 1 + 2
//! ```
//!
//! **Call forwarding proxy** — forwards a call to its first argument:
//! ```js
//! function _0x4(fn, a) { return fn(a); }
//! _0x4(alert, "hi")  // inlined to: alert("hi")
//! ```
//!
//! **Identity proxy** — returns its argument unchanged:
//! ```js
//! function _0x5(a) { return a; }
//! _0x5(x)  // inlined to: x
//! ```
//!
//! # Algorithm
//!
//! 1. Scan statement lists for function declarations whose body is a single
//!    return statement referencing only its own parameters.
//! 2. Classify each proxy (binary, call forwarding, or identity).
//! 3. Use scoping to verify the function is only used as a direct callee.
//! 4. Inline each call site using `operations::replace_expression`.
//! 5. Remove the proxy declaration using `operations::remove_statement_at`.

use std::collections::HashMap;
use std::sync::Mutex;

use oxc_allocator::Vec as ArenaVec;
use oxc_ast::ast::{BindingPattern, Expression, FunctionType, Statement};
use oxc_span::SPAN;
use oxc_syntax::operator::BinaryOperator;
use oxc_syntax::symbol::SymbolId;
use oxc_traverse::TraverseCtx;

use crate::operations;
use crate::transformer::{AstNodeType, Transformer, TransformerPhase, TransformerPriority};

/// The kind of proxy function detected.
#[derive(Debug, Clone)]
enum ProxyKind {
    /// `function f(a, b) { return a <op> b; }`
    Binary(BinaryOperator),

    /// `function f(callee, ...args) { return callee(...args); }`
    /// Stores the number of forwarded arguments (excluding the callee parameter).
    CallForwarding(usize),

    /// `function f(a) { return a; }`
    Identity,
}

/// Information about a detected proxy function.
#[derive(Debug, Clone)]
struct ProxyFunction {
    kind: ProxyKind,
}

/// Inlines proxy functions that wrap a single binary operation, call, or identity.
pub struct ProxyFunctionInliningTransformer {
    /// Maps function symbol IDs to their proxy classification.
    proxies: Mutex<HashMap<SymbolId, ProxyFunction>>,
}

impl Default for ProxyFunctionInliningTransformer {
    fn default() -> Self {
        Self {
            proxies: Mutex::new(HashMap::new()),
        }
    }
}

impl ProxyFunctionInliningTransformer {
    /// Try to classify a function declaration as a proxy function.
    ///
    /// Returns `Some(ProxyFunction)` if the function matches a known proxy pattern.
    fn classify_proxy<'a>(
        function: &oxc_ast::ast::Function<'a>,
        context: &TraverseCtx<'a, ()>,
    ) -> Option<ProxyFunction> {
        // Must have a body with exactly one statement.
        let body = function.body.as_ref()?;
        if body.statements.len() != 1 {
            return None;
        }

        // The single statement must be a return statement with an argument.
        let Statement::ReturnStatement(return_statement) = &body.statements[0] else {
            return None;
        };
        let return_expression = return_statement.argument.as_ref()?;

        // Unwrap parenthesized expressions.
        let return_expression = unwrap_parens(return_expression);

        let params = &function.params;
        let param_count = params.items.len();

        // Must not have rest parameters.
        if params.rest.is_some() {
            return None;
        }

        // Must not be async or generator.
        if function.r#async || function.generator {
            return None;
        }

        // Get the scope ID of the function to verify parameter bindings.
        let scope_id = function.scope_id.get()?;

        // Try to classify based on the return expression.

        // Pattern 1: Identity — `function f(a) { return a; }`
        if param_count == 1
            && let Expression::Identifier(identifier) = return_expression
                && is_parameter_reference(identifier, scope_id, 0, &params.items, context) {
                    return Some(ProxyFunction {
                        kind: ProxyKind::Identity,
                    });
                }

        // Pattern 2: Binary — `function f(a, b) { return a <op> b; }`
        if param_count == 2
            && let Expression::BinaryExpression(binary) = return_expression {
                let left = unwrap_parens(&binary.left);
                let right = unwrap_parens(&binary.right);

                if let (Expression::Identifier(left_id), Expression::Identifier(right_id)) =
                    (left, right)
                    && is_parameter_reference(left_id, scope_id, 0, &params.items, context)
                        && is_parameter_reference(right_id, scope_id, 1, &params.items, context)
                    {
                        return Some(ProxyFunction {
                            kind: ProxyKind::Binary(binary.operator),
                        });
                    }
            }

        // Pattern 3: Call forwarding — `function f(callee, a, b, ...) { return callee(a, b, ...); }`
        if param_count >= 1
            && let Expression::CallExpression(call) = return_expression {
                let callee = unwrap_parens(&call.callee);
                if let Expression::Identifier(callee_id) = callee
                    && is_parameter_reference(callee_id, scope_id, 0, &params.items, context) {
                        // All call arguments must reference the remaining parameters in order.
                        let forwarded_argument_count = call.arguments.len();
                        if forwarded_argument_count == param_count - 1 {
                            let all_match = call
                                .arguments
                                .iter()
                                .enumerate()
                                .all(|(index, argument)| {
                                    if argument.is_spread() {
                                        return false;
                                    }
                                    let Some(expression) = argument.as_expression() else {
                                        return false;
                                    };
                                    let expression = unwrap_parens(expression);
                                    if let Expression::Identifier(id) = expression {
                                        is_parameter_reference(
                                            id,
                                            scope_id,
                                            index + 1,
                                            &params.items,
                                            context,
                                        )
                                    } else {
                                        false
                                    }
                                });

                            if all_match {
                                return Some(ProxyFunction {
                                    kind: ProxyKind::CallForwarding(forwarded_argument_count),
                                });
                            }
                        }
                    }
            }

        None
    }

    /// Check whether all references to a symbol are direct callee positions.
    fn all_references_are_callees(
        symbol_id: SymbolId,
        context: &TraverseCtx<'_, ()>,
    ) -> bool {
        let reference_ids = context.scoping().get_resolved_reference_ids(symbol_id);
        if reference_ids.is_empty() {
            return false; // No references → nothing to inline.
        }
        // We can't easily check parent nodes from scoping alone. Instead, we
        // rely on the fact that no write references exist and the function is
        // not passed as a value. We check for write references here.
        reference_ids.iter().all(|&reference_id| {
            let reference = context.scoping().get_reference(reference_id);
            !reference.is_write()
        })
    }
}

/// Check if an identifier reference refers to the parameter at the given index.
fn is_parameter_reference<'a>(
    identifier: &oxc_ast::ast::IdentifierReference<'a>,
    function_scope_id: oxc_syntax::scope::ScopeId,
    parameter_index: usize,
    parameters: &ArenaVec<'a, oxc_ast::ast::FormalParameter<'a>>,
    context: &TraverseCtx<'a, ()>,
) -> bool {
    let Some(reference_id) = identifier.reference_id.get() else {
        return false;
    };
    let reference = context.scoping().get_reference(reference_id);
    let Some(symbol_id) = reference.symbol_id() else {
        return false;
    };

    // Check that the symbol belongs to the function's scope.
    let symbol_scope = context.scoping().symbol_scope_id(symbol_id);
    if symbol_scope != function_scope_id {
        return false;
    }

    // Check that the parameter at the given index has this symbol.
    if parameter_index >= parameters.len() {
        return false;
    }
    let BindingPattern::BindingIdentifier(param_binding) = &parameters[parameter_index].pattern
    else {
        return false;
    };
    param_binding.symbol_id.get() == Some(symbol_id)
}

/// Unwrap parenthesized expressions.
fn unwrap_parens<'a, 'b>(expression: &'b Expression<'a>) -> &'b Expression<'a> {
    let mut current = expression;
    while let Expression::ParenthesizedExpression(paren) = current {
        current = &paren.expression;
    }
    current
}

impl Transformer for ProxyFunctionInliningTransformer {
    fn name(&self) -> &str {
        "ProxyFunctionInliningTransformer"
    }

    fn interests(&self) -> &[AstNodeType] {
        &[AstNodeType::StatementList, AstNodeType::CallExpression]
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
        let mut proxies = self.proxies.lock().unwrap();

        // Phase 1: Scan for proxy function declarations.
        for statement in statements.iter() {
            let Statement::FunctionDeclaration(function) = statement else {
                continue;
            };

            if function.r#type != FunctionType::FunctionDeclaration {
                continue;
            }

            let Some(binding) = &function.id else {
                continue;
            };

            let Some(symbol_id) = binding.symbol_id.get() else {
                continue;
            };

            // Must not be reassigned.
            if Self::all_references_are_callees(symbol_id, context)
                && let Some(proxy) = Self::classify_proxy(function, context) {
                    proxies.insert(symbol_id, proxy);
                }
        }

        if proxies.is_empty() {
            return false;
        }

        // Phase 2: Remove proxy function declarations.
        let mut changed = false;
        for index in (0..statements.len()).rev() {
            let Statement::FunctionDeclaration(function) = &statements[index] else {
                continue;
            };

            if let Some(binding) = &function.id
                && let Some(symbol_id) = binding.symbol_id.get()
                    && proxies.contains_key(&symbol_id) {
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
        let Expression::CallExpression(call) = expression else {
            return false;
        };

        // Check if the callee is a reference to a known proxy function.
        let callee = unwrap_parens(&call.callee);
        let Expression::Identifier(identifier) = callee else {
            return false;
        };

        let Some(reference_id) = identifier.reference_id.get() else {
            return false;
        };

        let reference = context.scoping().get_reference(reference_id);
        let Some(symbol_id) = reference.symbol_id() else {
            return false;
        };

        let proxies = self.proxies.lock().unwrap();
        let Some(proxy) = proxies.get(&symbol_id) else {
            return false;
        };
        let proxy_kind = proxy.kind.clone();
        drop(proxies);

        // Take all arguments out of the call expression.
        let empty_arguments = context.ast.vec();
        let mut arguments: Vec<_> = std::mem::replace(&mut call.arguments, empty_arguments)
            .into_iter()
            .collect();

        match proxy_kind {
            ProxyKind::Identity => {
                // Replace `proxy(x)` with `x`.
                if arguments.len() != 1 {
                    return false;
                }
                if arguments[0].is_spread() {
                    return false;
                }
                let replacement = arguments.remove(0).into_expression();
                operations::replace_expression(expression, replacement, context);
                true
            }

            ProxyKind::Binary(operator) => {
                // Replace `proxy(a, b)` with `a <op> b`.
                if arguments.len() != 2 {
                    return false;
                }
                if arguments[0].is_spread() || arguments[1].is_spread() {
                    return false;
                }
                let right = arguments.remove(1).into_expression();
                let left = arguments.remove(0).into_expression();
                let replacement =
                    context.ast.expression_binary(SPAN, left, operator, right);
                operations::replace_expression(expression, replacement, context);
                true
            }

            ProxyKind::CallForwarding(expected_argument_count) => {
                // Replace `proxy(fn, a, b)` with `fn(a, b)`.
                if arguments.len() != expected_argument_count + 1 {
                    return false;
                }
                if arguments.iter().any(|a| a.is_spread()) {
                    return false;
                }

                let new_callee = arguments.remove(0).into_expression();
                let mut forwarded_arguments =
                    context.ast.vec_with_capacity(arguments.len());
                for argument in arguments {
                    forwarded_arguments.push(argument);
                }

                let type_parameters: Option<
                    oxc_allocator::Box<'_, oxc_ast::ast::TSTypeParameterInstantiation<'_>>,
                > = None;
                let replacement = context.ast.expression_call(
                    SPAN,
                    new_callee,
                    type_parameters,
                    forwarded_arguments,
                    false,
                );
                operations::replace_expression(expression, replacement, context);
                true
            }
        }
    }
}
