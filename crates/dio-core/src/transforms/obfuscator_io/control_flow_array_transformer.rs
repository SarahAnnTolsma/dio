//! Decodes and inlines control flow arrays used by obfuscation tools.
//!
//! Some obfuscators (e.g. Obfuscator.io) create a 2D array of object references
//! to flatten control flow. Switch/case statements compare values by reference
//! identity, making the code unreadable.
//!
//! This transformer detects the pattern, statically evaluates the hash function
//! to assign a unique numeric ID (0–31) to each distinct reference, and replaces
//! all double-indexed lookups with the computed numeric value.
//!
//! # Pattern
//!
//! ```js
//! // Hash function
//! function gn(n, t, e, i, a, o, r, s) {
//!     return (n * o ^ r * i ^ e * t) >>> 0 & a - 1;
//! }
//!
//! // IIFE builds a 2D array and assigns a row to `s`
//! var s;
//! !function(n, t) {
//!     var i = [];
//!     for (t = 0; t < 32; t++) i[t] = new Array(256);
//!     function a(n) {
//!         for (var t = 32 * n, a = Math.min(t + 32, 256), o = t; o < a; o++)
//!             for (e = 0; e < 32; e++)
//!                 i[e][o] = i[gn(o, C1, C2, C3, C4, C5, e)];
//!     }
//!     // ... schedule a(0)..a(7) and s = i[ROW_INDEX]
//! }(function(n) { setTimeout(n, 0); });
//!
//! // Usage: s[x][y] used in switch/case and comparisons
//! switch (t) {
//!     case s[71][87]: ...
//!     case s[49][145]: ...
//! }
//! ```

use std::collections::HashMap;
use std::sync::Mutex;

use oxc_allocator::Vec as ArenaVec;
use oxc_ast::ast::{Expression, FunctionType, Statement};
use oxc_span::SPAN;
use oxc_syntax::number::NumberBase;
use oxc_syntax::symbol::SymbolId;
use oxc_traverse::TraverseCtx;

use crate::operations;
use crate::transformer::{AstNodeType, Transformer, TransformerPhase, TransformerPriority};

/// Parameters extracted from the `gn` call site: `gn(o, C1, C2, C3, C4, C5, e)`.
#[derive(Debug, Clone)]
struct HashParameters {
    c1: u32,
    c2: u32,
    c3: u32,
    c4: u32,
    c5: u32,
}

/// Tracked control flow array variable.
#[derive(Debug, Clone)]
struct ControlFlowArray {
    /// The hash parameters from the `gn` call.
    parameters: HashParameters,
    /// Which row of the 2D array is assigned to the variable (e.g., 2 for `s = i[2]`).
    row_index: u32,
}

/// Decodes control flow array lookups and replaces them with numeric constants.
pub struct ControlFlowArrayTransformer {
    /// Maps the symbol ID of the control flow variable (e.g., `s`) to its parameters.
    arrays: Mutex<HashMap<SymbolId, ControlFlowArray>>,
}

impl Default for ControlFlowArrayTransformer {
    fn default() -> Self {
        Self {
            arrays: Mutex::new(HashMap::new()),
        }
    }
}

impl ControlFlowArrayTransformer {
    /// Evaluate the hash function: `(n * o ^ r * i ^ e * t) >>> 0 & (a - 1)`.
    ///
    /// Maps to `gn(n, t, e, i, a, o, r)` where:
    /// - n, t, e, i, a, o, r are the positional parameters
    /// - Call site: `gn(col, C1, C2, C3, C4, C5, row)`
    fn evaluate_hash(col: u32, params: &HashParameters, row: u32) -> u32 {
        // gn(n=col, t=C1, e=C2, i=C3, a=C4, o=C5, r=row)
        // return (n * o ^ r * i ^ e * t) >>> 0 & (a - 1)
        // = (col * C5 ^ row * C3 ^ C2 * C1) >>> 0 & (C4 - 1)
        let result = col.wrapping_mul(params.c5)
            ^ row.wrapping_mul(params.c3)
            ^ params.c2.wrapping_mul(params.c1);
        result & (params.c4 - 1)
    }

    /// Compute `s[x][y]` given the parameters and row index.
    ///
    /// `s = i[row_index]`, so `s[x] = i[row_index][x] = i[hash(x, row_index)]`.
    /// Then `s[x][y] = i[hash(x, row_index)][y] = i[hash(y, hash(x, row_index))]`.
    fn compute_value(x: u32, y: u32, array: &ControlFlowArray) -> u32 {
        let intermediate_row = Self::evaluate_hash(x, &array.parameters, array.row_index);
        Self::evaluate_hash(y, &array.parameters, intermediate_row)
    }
}

impl Transformer for ControlFlowArrayTransformer {
    fn name(&self) -> &str {
        "ControlFlowArrayTransformer"
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
        let mut arrays = self.arrays.lock().unwrap();
        if !arrays.is_empty() {
            return false;
        }

        // Look for the hash function: function gn(n, t, e, i, a, o, r, s) {
        //     return (n * o ^ r * i ^ e * t) >>> 0 & a - 1;
        // }
        let mut hash_function_symbol: Option<SymbolId> = None;
        for statement in statements.iter() {
            let Statement::FunctionDeclaration(function) = statement else {
                continue;
            };
            if function.r#type != FunctionType::FunctionDeclaration {
                continue;
            }
            if !is_hash_function(function) {
                continue;
            }
            if let Some(binding) = &function.id
                && let Some(symbol_id) = binding.symbol_id.get()
            {
                hash_function_symbol = Some(symbol_id);
                break;
            }
        }

        let Some(hash_symbol) = hash_function_symbol else {
            return false;
        };

        // Scan for the IIFE that builds the 2D array and assigns to a variable.
        // Pattern: the IIFE contains a call to the hash function with constants,
        // and assigns a row of the array to a variable.
        if let Some((target_symbol, array_info)) =
            find_control_flow_iife(statements, hash_symbol, context)
        {
            arrays.insert(target_symbol, array_info);
        }

        if arrays.is_empty() {
            return false;
        }

        // Remove the hash function declaration.
        let mut changed = false;
        for index in (0..statements.len()).rev() {
            if let Statement::FunctionDeclaration(function) = &statements[index]
                && let Some(binding) = &function.id
                && let Some(symbol_id) = binding.symbol_id.get()
                && symbol_id == hash_symbol
            {
                operations::remove_statement_at(statements, index, context);
                changed = true;
            }
        }

        // Remove the IIFE (expression statement containing the !function(){}() call).
        for index in (0..statements.len()).rev() {
            if is_control_flow_iife_statement(&statements[index], hash_symbol, context) {
                operations::remove_statement_at(statements, index, context);
                changed = true;
            }
        }

        // Remove the `var s;` declaration if it has no initializer.
        let tracked_symbols: Vec<SymbolId> = arrays.keys().copied().collect();
        for index in (0..statements.len()).rev() {
            let Statement::VariableDeclaration(declaration) = &statements[index] else {
                continue;
            };
            if declaration.declarations.len() != 1 {
                continue;
            }
            let declarator = &declaration.declarations[0];
            if declarator.init.is_some() {
                continue;
            }
            if let oxc_ast::ast::BindingPattern::BindingIdentifier(binding) = &declarator.id
                && let Some(symbol_id) = binding.symbol_id.get()
                && tracked_symbols.contains(&symbol_id)
            {
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
        let arrays = self.arrays.lock().unwrap();
        if arrays.is_empty() {
            return false;
        }

        // Match s[x][y] — a ComputedMemberExpression where:
        // - The outer index (y) is a numeric literal or null
        // - The object is a ComputedMemberExpression where:
        //   - The inner index (x) is a numeric literal or null
        //   - The object is an identifier matching a tracked variable
        let Expression::ComputedMemberExpression(outer) = expression else {
            return false;
        };

        let outer_index = extract_array_index(&outer.expression);
        if outer_index.is_none() && !matches!(&outer.expression, Expression::NullLiteral(_)) {
            return false;
        }

        let Expression::ComputedMemberExpression(inner) = &outer.object else {
            return false;
        };

        let inner_index = extract_array_index(&inner.expression);
        if inner_index.is_none() && !matches!(&inner.expression, Expression::NullLiteral(_)) {
            return false;
        }

        let Expression::Identifier(identifier) = &inner.object else {
            return false;
        };

        let Some(reference_id) = identifier.reference_id.get() else {
            return false;
        };

        let reference = context.scoping().get_reference(reference_id);
        let Some(symbol_id) = reference.symbol_id() else {
            return false;
        };

        if !arrays.contains_key(&symbol_id) {
            return false;
        }

        // If either index is null, the access is s["null"] which is undefined
        // for a numeric-indexed array. Replace with void 0.
        if inner_index.is_none() || outer_index.is_none() {
            let replacement = context.ast.void_0(SPAN);
            operations::replace_expression(expression, replacement, context);
            return true;
        }

        let array = &arrays[&symbol_id];
        let value = Self::compute_value(inner_index.unwrap(), outer_index.unwrap(), array);
        let raw = context.ast.atom(&value.to_string());
        let replacement = context.ast.expression_numeric_literal(
            SPAN,
            f64::from(value),
            Some(raw),
            NumberBase::Decimal,
        );
        operations::replace_expression(expression, replacement, context);
        true
    }
}

/// Extract a u32 index from a numeric literal or a string literal that parses to one.
fn extract_array_index(expression: &Expression<'_>) -> Option<u32> {
    match expression {
        Expression::NumericLiteral(number) => {
            let value = number.value;
            if (0.0..256.0).contains(&value) && value.fract() == 0.0 {
                Some(value as u32)
            } else {
                None
            }
        }
        Expression::StringLiteral(string) => string.value.as_str().parse::<u32>().ok(),
        _ => None,
    }
}

/// Check if a function matches the hash function pattern:
/// `function gn(n, t, e, i, a, o, r, s) { return (n * o ^ r * i ^ e * t) >>> 0 & a - 1; }`
///
/// We check structural properties: 7-8 parameters, single return statement with
/// a bitwise unsigned-right-shift and AND operation.
fn is_hash_function(function: &oxc_ast::ast::Function<'_>) -> bool {
    // Must have 7 or 8 parameters.
    if function.params.items.len() < 7 {
        return false;
    }

    let Some(body) = &function.body else {
        return false;
    };

    // Must have exactly one statement: a return.
    if body.statements.len() != 1 {
        return false;
    }

    let Statement::ReturnStatement(return_statement) = &body.statements[0] else {
        return false;
    };

    let Some(argument) = &return_statement.argument else {
        return false;
    };

    // The return expression should be: (...) >>> 0 & (a - 1)
    // Which is parsed as: ((...) >>> 0) & (a - 1)
    // Check for a BinaryExpression with BitwiseAnd.
    let Expression::BinaryExpression(and_expression) = argument else {
        return false;
    };

    if and_expression.operator != oxc_syntax::operator::BinaryOperator::BitwiseAnd {
        return false;
    }

    // Left side should be (...) >>> 0
    let Expression::BinaryExpression(shift_expression) = &and_expression.left else {
        return false;
    };

    if shift_expression.operator != oxc_syntax::operator::BinaryOperator::ShiftRightZeroFill {
        return false;
    }

    // Right of >>> should be 0
    let Expression::NumericLiteral(zero) = &shift_expression.right else {
        return false;
    };
    if zero.value != 0.0 {
        return false;
    }

    // Left of >>> should contain XOR operations with multiplications
    // (n * o ^ r * i ^ e * t)
    has_xor_multiply_pattern(&shift_expression.left)
}

/// Check if an expression contains XOR and multiply operations (loose check).
fn has_xor_multiply_pattern(expression: &Expression<'_>) -> bool {
    match expression {
        Expression::BinaryExpression(binary) => {
            if binary.operator == oxc_syntax::operator::BinaryOperator::BitwiseXOR {
                return true;
            }
            has_xor_multiply_pattern(&binary.left) || has_xor_multiply_pattern(&binary.right)
        }
        Expression::ParenthesizedExpression(paren) => has_xor_multiply_pattern(&paren.expression),
        _ => false,
    }
}

/// Find the control flow IIFE and extract the target variable symbol and parameters.
///
/// Looks for:
/// 1. An IIFE containing a call to the hash function with numeric constants
/// 2. An assignment `s = i[ROW_INDEX]` inside the IIFE
fn find_control_flow_iife<'a>(
    statements: &ArenaVec<'a, Statement<'a>>,
    hash_symbol: SymbolId,
    context: &TraverseCtx<'a, ()>,
) -> Option<(SymbolId, ControlFlowArray)> {
    for statement in statements.iter() {
        // The IIFE is an expression statement: !function(n, t) { ... }(callback)
        let Statement::ExpressionStatement(expression_statement) = statement else {
            continue;
        };

        let iife_body = extract_iife_body(&expression_statement.expression)?;

        // Search the IIFE body for:
        // 1. A call to the hash function with constants
        // 2. An assignment to the target variable
        let parameters = find_hash_call_parameters(iife_body, hash_symbol, context)?;

        // Find `s = i[ROW_INDEX]` — look for an assignment expression inside a
        // callback passed to n() (the scheduler).
        let (target_symbol, row_index) = find_target_assignment(iife_body, context)?;

        return Some((
            target_symbol,
            ControlFlowArray {
                parameters,
                row_index,
            },
        ));
    }

    None
}

/// Extract the body statements of an IIFE from various forms:
/// - `!function(...) { BODY }(args)` (unary not + call)
/// - `(function(...) { BODY })(args)`
fn extract_iife_body<'a>(expression: &'a Expression<'a>) -> Option<&'a [Statement<'a>]> {
    // !function(...) { ... }(args)
    if let Expression::UnaryExpression(unary) = expression
        && let Expression::CallExpression(call) = &unary.argument
        && let Expression::FunctionExpression(function) = &call.callee
        && let Some(body) = &function.body
    {
        return Some(&body.statements);
    }

    // (function(...) { ... })(args)
    if let Expression::CallExpression(call) = expression {
        let callee = match &call.callee {
            Expression::ParenthesizedExpression(paren) => &paren.expression,
            other => other,
        };
        if let Expression::FunctionExpression(function) = callee
            && let Some(body) = &function.body
        {
            return Some(&body.statements);
        }
    }

    None
}

/// Search for a call to the hash function and extract the constant parameters.
///
/// Looking for: `gn(o, C1, C2, C3, C4, C5, e)` inside a nested function.
fn find_hash_call_parameters(
    statements: &[Statement<'_>],
    hash_symbol: SymbolId,
    context: &TraverseCtx<'_, ()>,
) -> Option<HashParameters> {
    for statement in statements {
        // Look in function declarations inside the IIFE.
        if let Statement::FunctionDeclaration(function) = statement
            && let Some(body) = &function.body
            && let Some(params) =
                find_hash_call_in_statements(&body.statements, hash_symbol, context)
        {
            return Some(params);
        }
    }
    None
}

/// Recursively search statements for a call to the hash function.
fn find_hash_call_in_statements(
    statements: &[Statement<'_>],
    hash_symbol: SymbolId,
    context: &TraverseCtx<'_, ()>,
) -> Option<HashParameters> {
    for statement in statements {
        match statement {
            Statement::ForStatement(for_statement) => {
                if let Statement::BlockStatement(body) = &for_statement.body
                    && let Some(params) =
                        find_hash_call_in_statements(&body.body, hash_symbol, context)
                {
                    return Some(params);
                }
            }
            Statement::ExpressionStatement(expression_statement) => {
                if let Some(params) = find_hash_call_in_expression(
                    &expression_statement.expression,
                    hash_symbol,
                    context,
                ) {
                    return Some(params);
                }
            }
            _ => {}
        }
    }
    None
}

/// Check if an expression contains a call to the hash function and extract parameters.
fn find_hash_call_in_expression(
    expression: &Expression<'_>,
    hash_symbol: SymbolId,
    context: &TraverseCtx<'_, ()>,
) -> Option<HashParameters> {
    match expression {
        Expression::AssignmentExpression(assignment) => {
            // i[e][o] = i[gn(o, C1, C2, C3, C4, C5, e)]
            find_hash_call_in_expression(&assignment.right, hash_symbol, context)
        }
        Expression::ComputedMemberExpression(computed) => {
            // i[gn(...)]
            find_hash_call_in_expression(&computed.expression, hash_symbol, context)
                .or_else(|| find_hash_call_in_expression(&computed.object, hash_symbol, context))
        }
        Expression::CallExpression(call) => {
            // gn(o, C1, C2, C3, C4, C5, e)
            let Expression::Identifier(callee) = &call.callee else {
                return None;
            };
            let reference_id = callee.reference_id.get()?;
            let reference = context.scoping().get_reference(reference_id);
            let callee_symbol = reference.symbol_id()?;
            if callee_symbol != hash_symbol {
                return None;
            }

            // Expected: gn(var, C1, C2, C3, C4, C5, var)
            // Arguments 1-5 (indices 1..=5) should be numeric literals.
            if call.arguments.len() < 7 {
                return None;
            }

            let c1 = extract_argument_number(&call.arguments[1])?;
            let c2 = extract_argument_number(&call.arguments[2])?;
            let c3 = extract_argument_number(&call.arguments[3])?;
            let c4 = extract_argument_number(&call.arguments[4])?;
            let c5 = extract_argument_number(&call.arguments[5])?;

            // C4 must be a power of 2 greater than 1 (used as bitmask: & (C4 - 1)).
            if c4 < 2 || (c4 & (c4 - 1)) != 0 {
                return None;
            }

            Some(HashParameters { c1, c2, c3, c4, c5 })
        }
        _ => None,
    }
}

/// Extract a numeric value from a call argument.
fn extract_argument_number(argument: &oxc_ast::ast::Argument<'_>) -> Option<u32> {
    let expression = argument.as_expression()?;
    if let Expression::NumericLiteral(number) = expression {
        Some(number.value as u32)
    } else {
        None
    }
}

/// Find the assignment `s = i[ROW_INDEX]` inside the IIFE body.
///
/// This is typically inside a callback: `n(function() { s = i[2]; })`
fn find_target_assignment(
    statements: &[Statement<'_>],
    context: &TraverseCtx<'_, ()>,
) -> Option<(SymbolId, u32)> {
    for statement in statements {
        // Look for expression statements containing n(function() { s = i[ROW]; })
        let Statement::ExpressionStatement(expression_statement) = statement else {
            continue;
        };

        let Expression::CallExpression(call) = &expression_statement.expression else {
            continue;
        };

        // The argument should be a function expression.
        if call.arguments.len() != 1 {
            continue;
        }
        let Some(argument) = call.arguments[0].as_expression() else {
            continue;
        };
        let Expression::FunctionExpression(function) = argument else {
            continue;
        };
        let Some(body) = &function.body else {
            continue;
        };

        // Look for `s = i[ROW_INDEX]` in the function body.
        for inner_statement in &body.statements {
            let Statement::ExpressionStatement(inner_expression) = inner_statement else {
                continue;
            };
            let Expression::AssignmentExpression(assignment) = &inner_expression.expression else {
                continue;
            };
            if assignment.operator != oxc_syntax::operator::AssignmentOperator::Assign {
                continue;
            }

            // Right side: i[ROW_INDEX]
            let Expression::ComputedMemberExpression(computed) = &assignment.right else {
                continue;
            };
            let Some(row_index) = extract_array_index(&computed.expression) else {
                continue;
            };

            // Left side: s (identifier)
            let oxc_ast::ast::AssignmentTarget::AssignmentTargetIdentifier(target) =
                &assignment.left
            else {
                continue;
            };
            let Some(reference_id) = target.reference_id.get() else {
                continue;
            };
            let reference = context.scoping().get_reference(reference_id);
            let Some(symbol_id) = reference.symbol_id() else {
                continue;
            };

            return Some((symbol_id, row_index));
        }
    }

    None
}

/// Check if a statement is the control flow IIFE (for removal).
fn is_control_flow_iife_statement(
    statement: &Statement<'_>,
    hash_symbol: SymbolId,
    context: &TraverseCtx<'_, ()>,
) -> bool {
    let Statement::ExpressionStatement(expression_statement) = statement else {
        return false;
    };

    let Some(body) = extract_iife_body(&expression_statement.expression) else {
        return false;
    };

    // Check if this IIFE contains a call to the hash function.
    find_hash_call_parameters(body, hash_symbol, context).is_some()
}
