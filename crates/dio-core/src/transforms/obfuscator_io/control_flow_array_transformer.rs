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

/// Parameters for evaluating the control flow hash function.
///
/// The hash is always `(A*B ^ C*D ^ E*F) >>> 0 & (G - 1)` with varying
/// parameter orderings. We store the constant argument values and the
/// positions of the two variable arguments (row/col loop variables).
/// At evaluation time, we substitute row/col and compute the result.
#[derive(Debug, Clone)]
struct HashParameters {
    /// All call-site argument values. Variable positions store 0 as placeholder.
    call_args: Vec<u32>,
    /// Index in call_args for the column variable (outer loop var).
    col_arg_index: usize,
    /// Index in call_args for the row variable (inner loop var).
    row_arg_index: usize,
    /// Indices of the three multiply pairs in the function parameters,
    /// mapped to call argument indices. Each pair (a_idx, b_idx) means
    /// call_args[a_idx] * call_args[b_idx] in the XOR chain.
    multiply_pairs: [(usize, usize); 3],
    /// Call argument index used for the mask: `& (call_args[mask_idx] - 1)`.
    mask_arg_index: usize,
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
    /// Evaluate the hash function by substituting col/row into the call args
    /// and computing `(A*B ^ C*D ^ E*F) >>> 0 & (G - 1)`.
    fn evaluate_hash(col: u32, params: &HashParameters, row: u32) -> u32 {
        let mut args = params.call_args.clone();
        args[params.col_arg_index] = col;
        args[params.row_arg_index] = row;

        let [(a1, b1), (a2, b2), (a3, b3)] = params.multiply_pairs;
        let result = args[a1].wrapping_mul(args[b1])
            ^ args[a2].wrapping_mul(args[b2])
            ^ args[a3].wrapping_mul(args[b3]);
        result & (args[params.mask_arg_index] - 1)
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
        let mut hash_function_index: Option<usize> = None;
        for (stmt_idx, statement) in statements.iter().enumerate() {
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
                hash_function_index = Some(stmt_idx);
                break;
            }
        }

        let Some(hash_symbol) = hash_function_symbol else {
            return false;
        };

        // Scan for the IIFE that builds the 2D array and assigns to a variable.
        // Pattern: the IIFE contains a call to the hash function with constants,
        // and assigns a row of the array to a variable.
        let hash_func_idx = hash_function_index.unwrap();
        let Statement::FunctionDeclaration(hash_func) = &statements[hash_func_idx] else {
            return false;
        };

        // Extract hash structure before we drop the immutable borrow.
        let hash_body_info = extract_hash_body_structure(hash_func);
        let hash_param_names: Vec<String> = hash_func
            .params
            .items
            .iter()
            .filter_map(|p| {
                if let oxc_ast::ast::BindingPattern::BindingIdentifier(b) = &p.pattern {
                    Some(b.name.to_string())
                } else {
                    None
                }
            })
            .collect();

        if let Some((target_symbol, array_info)) = find_control_flow_iife(
            statements,
            hash_symbol,
            &hash_param_names,
            &hash_body_info,
            context,
        ) {
            arrays.insert(target_symbol, array_info);
        }

        if arrays.is_empty() {
            return false;
        }

        // Remove the hash function declaration and the IIFE.
        let mut changed = false;
        for index in (0..statements.len()).rev() {
            match &statements[index] {
                Statement::FunctionDeclaration(function) => {
                    if let Some(binding) = &function.id
                        && let Some(sym) = binding.symbol_id.get()
                        && sym == hash_symbol
                    {
                        operations::remove_statement_at(statements, index, context);
                        changed = true;
                    }
                }
                Statement::ExpressionStatement(expr_stmt) => {
                    if let Some(body) = extract_iife_body(&expr_stmt.expression)
                        && find_hash_call_parameters(
                            body,
                            hash_symbol,
                            &hash_param_names,
                            &hash_body_info,
                            context,
                        )
                        .is_some()
                    {
                        operations::remove_statement_at(statements, index, context);
                        changed = true;
                    }
                }
                _ => {}
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
    hash_param_names: &[String],
    hash_body_info: &Option<(Vec<(String, String)>, String)>,
    context: &TraverseCtx<'a, ()>,
) -> Option<(SymbolId, ControlFlowArray)> {
    for statement in statements.iter() {
        let Statement::ExpressionStatement(expression_statement) = statement else {
            continue;
        };

        let iife_body = extract_iife_body(&expression_statement.expression)?;

        let mut parameters = find_hash_call_parameters(
            iife_body,
            hash_symbol,
            hash_param_names,
            hash_body_info,
            context,
        )?;

        let (target_symbol, row_index) = find_target_assignment(iife_body, context)?;

        // The col/row assignment may be swapped. Try both orderings and
        // pick the one that's consistent (produces only 32 distinct values
        // for all possible inputs, matching the mask size).
        // Quick validation: compute a[0][0] and a[0][1] — they should differ.
        let test_a = ControlFlowArrayTransformer::compute_value(
            0,
            0,
            &ControlFlowArray {
                parameters: parameters.clone(),
                row_index,
            },
        );
        let test_b = ControlFlowArrayTransformer::compute_value(
            0,
            1,
            &ControlFlowArray {
                parameters: parameters.clone(),
                row_index,
            },
        );
        if test_a == test_b {
            // All values for different y produce the same result — col/row are swapped.
            std::mem::swap(&mut parameters.col_arg_index, &mut parameters.row_arg_index);
        }

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
    hash_param_names: &[String],
    hash_body_info: &Option<(Vec<(String, String)>, String)>,
    context: &TraverseCtx<'_, ()>,
) -> Option<HashParameters> {
    for statement in statements {
        // Look in function declarations inside the IIFE.
        if let Statement::FunctionDeclaration(function) = statement
            && let Some(body) = &function.body
            && let Some(params) = find_hash_call_in_statements(
                &body.statements,
                hash_symbol,
                hash_param_names,
                hash_body_info,
                context,
            )
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
    hash_param_names: &[String],
    hash_body_info: &Option<(Vec<(String, String)>, String)>,
    context: &TraverseCtx<'_, ()>,
) -> Option<HashParameters> {
    for statement in statements {
        match statement {
            Statement::ForStatement(for_statement) => {
                if let Statement::BlockStatement(body) = &for_statement.body
                    && let Some(params) = find_hash_call_in_statements(
                        &body.body,
                        hash_symbol,
                        hash_param_names,
                        hash_body_info,
                        context,
                    )
                {
                    return Some(params);
                }
            }
            Statement::ExpressionStatement(expression_statement) => {
                if let Some(params) = find_hash_call_in_expression(
                    &expression_statement.expression,
                    hash_symbol,
                    hash_param_names,
                    hash_body_info,
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
    hash_param_names: &[String],
    hash_body_info: &Option<(Vec<(String, String)>, String)>,
    context: &TraverseCtx<'_, ()>,
) -> Option<HashParameters> {
    match expression {
        Expression::AssignmentExpression(assignment) => find_hash_call_in_expression(
            &assignment.right,
            hash_symbol,
            hash_param_names,
            hash_body_info,
            context,
        ),
        Expression::ComputedMemberExpression(computed) => find_hash_call_in_expression(
            &computed.expression,
            hash_symbol,
            hash_param_names,
            hash_body_info,
            context,
        )
        .or_else(|| {
            find_hash_call_in_expression(
                &computed.object,
                hash_symbol,
                hash_param_names,
                hash_body_info,
                context,
            )
        }),
        Expression::CallExpression(call) => {
            let Expression::Identifier(callee) = &call.callee else {
                return None;
            };
            let reference_id = callee.reference_id.get()?;
            let reference = context.scoping().get_reference(reference_id);
            let callee_symbol = reference.symbol_id()?;
            if callee_symbol != hash_symbol {
                return None;
            }

            if call.arguments.len() < 7 {
                return None;
            }

            // Collect all call arguments: identify constants (numeric literals)
            // and variables (identifiers — the row/col loop vars).
            let mut call_args: Vec<u32> = Vec::new();
            let mut var_indices: Vec<usize> = Vec::new();
            for (idx, arg) in call.arguments.iter().enumerate() {
                if let Some(value) = extract_argument_number(arg) {
                    call_args.push(value);
                } else {
                    // Variable argument (row or col).
                    call_args.push(0); // placeholder
                    var_indices.push(idx);
                }
            }

            // Must have exactly 2 variable arguments (col and row).
            if var_indices.len() != 2 {
                return None;
            }

            // Use the pre-extracted hash structure.
            let (multiply_pairs, mask_param_name) = hash_body_info.as_ref()?;

            // Map parameter names to call argument indices.
            let name_to_arg_idx =
                |name: &str| -> Option<usize> { hash_param_names.iter().position(|p| p == name) };

            let pairs: [(usize, usize); 3] = [
                (
                    name_to_arg_idx(&multiply_pairs[0].0)?,
                    name_to_arg_idx(&multiply_pairs[0].1)?,
                ),
                (
                    name_to_arg_idx(&multiply_pairs[1].0)?,
                    name_to_arg_idx(&multiply_pairs[1].1)?,
                ),
                (
                    name_to_arg_idx(&multiply_pairs[2].0)?,
                    name_to_arg_idx(&multiply_pairs[2].1)?,
                ),
            ];
            let mask_arg_index = name_to_arg_idx(mask_param_name)?;

            // Validate the mask value is a power of 2 > 1.
            let mask_value = call_args[mask_arg_index];
            if mask_value < 2 || (mask_value & (mask_value - 1)) != 0 {
                return None;
            }

            // Determine which variable is col (outer loop) and which is row (inner loop).
            // In the IIFE: `for (o = ...) { for (r = ...) { t[r][o] = t[hash(...)]; } }`
            // The first variable arg in the call is typically col, second is row,
            // but we need to check which one maps to the inner vs outer loop.
            // For safety, try both orderings — the IIFE structure will validate.
            let col_arg_index = var_indices[0];
            let row_arg_index = var_indices[1];

            Some(HashParameters {
                call_args,
                col_arg_index,
                row_arg_index,
                multiply_pairs: pairs,
                mask_arg_index,
            })
        }
        _ => None,
    }
}

/// Extract the multiply-XOR-mask structure from the hash function body.
///
/// Expects: `return (A*B ^ C*D ^ E*F) >>> 0 & (G - 1);`
/// Returns the three multiply pairs and the mask parameter name.
fn extract_hash_body_structure(
    function: &oxc_ast::ast::Function<'_>,
) -> Option<(Vec<(String, String)>, String)> {
    let body = function.body.as_ref()?;
    if body.statements.len() != 1 {
        return None;
    }
    let Statement::ReturnStatement(ret) = &body.statements[0] else {
        return None;
    };
    let argument = ret.argument.as_ref()?;

    // Pattern: (...) >>> 0 & (G - 1)
    let Expression::BinaryExpression(and_expr) = argument else {
        return None;
    };
    if and_expr.operator != oxc_syntax::operator::BinaryOperator::BitwiseAnd {
        return None;
    }

    // Right side: G - 1 or just G
    let mask_param = extract_mask_param(&and_expr.right)?;

    // Left side: (...) >>> 0
    let Expression::BinaryExpression(shift_expr) = &and_expr.left else {
        return None;
    };
    if shift_expr.operator != oxc_syntax::operator::BinaryOperator::ShiftRightZeroFill {
        return None;
    }

    // Inner: A*B ^ C*D ^ E*F
    let mut pairs = Vec::new();
    collect_multiply_pairs(&shift_expr.left, &mut pairs);

    if pairs.len() != 3 {
        return None;
    }

    Some((pairs, mask_param))
}

/// Extract the mask parameter name from `G - 1` expression.
fn extract_mask_param(expression: &Expression<'_>) -> Option<String> {
    if let Expression::BinaryExpression(binary) = expression
        && binary.operator == oxc_syntax::operator::BinaryOperator::Subtraction
        && let Expression::Identifier(id) = &binary.left
    {
        return Some(id.name.to_string());
    }
    // Could be just an identifier if the subtraction was already folded.
    if let Expression::Identifier(id) = expression {
        return Some(id.name.to_string());
    }
    None
}

/// Recursively collect multiply pairs from a XOR chain: A*B ^ C*D ^ E*F.
fn collect_multiply_pairs(expression: &Expression<'_>, pairs: &mut Vec<(String, String)>) {
    match expression {
        Expression::BinaryExpression(binary)
            if binary.operator == oxc_syntax::operator::BinaryOperator::BitwiseXOR =>
        {
            collect_multiply_pairs(&binary.left, pairs);
            collect_multiply_pairs(&binary.right, pairs);
        }
        Expression::BinaryExpression(binary)
            if binary.operator == oxc_syntax::operator::BinaryOperator::Multiplication =>
        {
            if let (Expression::Identifier(left), Expression::Identifier(right)) =
                (&binary.left, &binary.right)
            {
                pairs.push((left.name.to_string(), right.name.to_string()));
            }
        }
        Expression::ParenthesizedExpression(paren) => {
            collect_multiply_pairs(&paren.expression, pairs);
        }
        _ => {}
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
