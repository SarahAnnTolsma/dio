//! Decodes and inlines RC4-encrypted string array lookups.
//!
//! Handles the Obfuscator.io "high" obfuscation pattern where:
//! 1. A self-replacing function returns an array of base64+RC4-encoded strings
//! 2. A decoder function takes (index, key), base64-decodes, then RC4-decrypts
//! 3. An IIFE rotates the array until a checksum matches
//! 4. Wrapper functions throughout the code call the decoder with offsets
//!
//! # Pattern
//!
//! ```js
//! function _0x3b41() {
//!     var arr = ["WOXYW5Daxq", ...];
//!     _0x3b41 = function() { return arr; };
//!     return _0x3b41();
//! }
//! function _0x1a5c(index, key) {
//!     index = index - 442;
//!     var arr = _0x3b41();
//!     // ... base64 decode + RC4 decrypt with key ...
//!     return decrypted;
//! }
//! // Rotation IIFE
//! (function(getArr, target) {
//!     var arr = getArr();
//!     while (true) {
//!         try {
//!             var sum = parseInt(wrapper1(...)) / 1 + ...;
//!             if (sum === target) break;
//!             else arr.push(arr.shift());
//!         } catch(e) { arr.push(arr.shift()); }
//!     }
//! })(_0x3b41, 865031);
//!
//! // Wrapper functions
//! function _0x5d34f5(a, b, c, d, e) { return _0x1a5c(d - 512, e); }
//! ```

use std::collections::HashMap;
use std::sync::Mutex;

use oxc_allocator::Vec as ArenaVec;
use oxc_ast::ast::{Expression, FunctionType, Statement};
use oxc_ast_visit::{Visit, walk};
use oxc_span::SPAN;
use oxc_syntax::symbol::SymbolId;
use oxc_traverse::TraverseCtx;

use crate::operations;
use crate::transformer::{AstNodeType, Transformer, TransformerPhase, TransformerPriority};
use crate::utils::unwrap_parens;

/// How a wrapper function maps its parameters to the decoder call.
#[derive(Debug, Clone)]
struct WrapperMapping {
    /// Index of the wrapper parameter used for the decoder's first arg (index).
    index_parameter: usize,
    /// Arithmetic offset applied: effective_index = param + offset.
    index_offset: i64,
    /// Index of the wrapper parameter used for the decoder's second arg (RC4 key).
    key_parameter: usize,
}

/// Information about the RC4 decoder and its resolved array.
#[derive(Debug, Clone)]
struct RC4DecoderInfo {
    /// The rotated array of encoded strings.
    array: Vec<String>,
    /// The base offset subtracted from the index (e.g., 442).
    base_offset: usize,
}

/// Entry in the decoders map — either the main decoder or a wrapper.
#[derive(Debug, Clone)]
enum DecoderEntry {
    /// The main RC4 decoder function.
    Decoder(RC4DecoderInfo),
    /// A wrapper function that forwards to the decoder with parameter remapping.
    Wrapper {
        mapping: WrapperMapping,
        decoder_info: RC4DecoderInfo,
    },
}

/// Decodes RC4-encrypted string array lookups.
pub struct StringArrayRC4DecoderTransformer {
    /// Maps function SymbolId -> decoder entry (main decoder or wrapper).
    decoders: Mutex<HashMap<SymbolId, DecoderEntry>>,
}

impl Default for StringArrayRC4DecoderTransformer {
    fn default() -> Self {
        Self {
            decoders: Mutex::new(HashMap::new()),
        }
    }
}

impl Transformer for StringArrayRC4DecoderTransformer {
    fn name(&self) -> &str {
        "StringArrayRC4DecoderTransformer"
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
        let mut decoders = self.decoders.lock().unwrap();
        if !decoders.is_empty() {
            // Already detected — scan for wrapper functions in nested scopes.
            return self.find_wrappers_in_scope(statements, &mut decoders, context);
        }

        // Step 1: Find self-replacing array functions.
        let mut array_functions: Vec<(SymbolId, Vec<String>)> = Vec::new();
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
            if let Some(elements) = extract_self_replacing_array(function) {
                array_functions.push((symbol_id, elements));
            }
        }

        if array_functions.is_empty() {
            return false;
        }

        // Step 2: Find RC4 decoder functions.
        let mut decoder_info: Option<(SymbolId, usize, usize)> = None; // (decoder_sym, array_idx, offset)
        for statement in statements.iter() {
            let Statement::FunctionDeclaration(function) = statement else {
                continue;
            };
            if function.r#type != FunctionType::FunctionDeclaration {
                continue;
            };
            // RC4 decoders have 2 parameters.
            if function.params.items.len() != 2 {
                continue;
            }
            let Some(binding) = &function.id else {
                continue;
            };
            let Some(decoder_symbol) = binding.symbol_id.get() else {
                continue;
            };

            if let Some((array_func_idx, offset)) =
                classify_rc4_decoder(function, &array_functions, context)
            {
                decoder_info = Some((decoder_symbol, array_func_idx, offset));
                break;
            }
        }

        let Some((decoder_symbol, array_func_idx, base_offset)) = decoder_info else {
            return false;
        };

        let raw_array = array_functions[array_func_idx].1.clone();
        let array_func_symbol = array_functions[array_func_idx].0;

        // Step 3: Find rotation IIFE and solve the rotation.
        let mut rotated_array = raw_array;
        for statement in statements.iter() {
            if let Some(target) =
                extract_rotation_iife(statement, array_func_symbol, context)
            {
                if let Some(solved) = solve_rotation_rc4(
                    &rotated_array,
                    base_offset,
                    target,
                    statement,
                    decoder_symbol,
                    context,
                ) {
                    rotated_array = solved;
                    break;
                }
            }
        }

        let info = RC4DecoderInfo {
            array: rotated_array,
            base_offset,
        };
        decoders.insert(decoder_symbol, DecoderEntry::Decoder(info.clone()));

        // Step 4: Find wrapper functions in this scope.
        self.find_wrappers_in_scope(statements, &mut decoders, context);

        // Step 5: Remove the array function, decoder function, and rotation IIFE.
        let mut changed = false;
        for index in (0..statements.len()).rev() {
            match &statements[index] {
                Statement::FunctionDeclaration(function) => {
                    if let Some(binding) = &function.id {
                        if let Some(sym) = binding.symbol_id.get() {
                            if sym == array_func_symbol || sym == decoder_symbol {
                                operations::remove_statement_at(statements, index, context);
                                changed = true;
                            }
                        }
                    }
                }
                Statement::ExpressionStatement(_) => {
                    if extract_rotation_iife(&statements[index], array_func_symbol, context)
                        .is_some()
                    {
                        operations::remove_statement_at(statements, index, context);
                        changed = true;
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
        let decoders = self.decoders.lock().unwrap();
        if decoders.is_empty() {
            return false;
        }

        let Expression::CallExpression(call) = expression else {
            return false;
        };

        let Expression::Identifier(callee) = &call.callee else {
            return false;
        };

        let Some(reference_id) = callee.reference_id.get() else {
            return false;
        };
        let reference = context.scoping().get_reference(reference_id);
        let Some(symbol_id) = reference.symbol_id() else {
            return false;
        };

        let Some(entry) = decoders.get(&symbol_id) else {
            return false;
        };

        let (info, effective_index, key) = match entry {
            DecoderEntry::Decoder(info) => {
                // Direct call: _0x1a5c(numericLiteral, stringLiteral)
                if call.arguments.len() != 2 {
                    return false;
                }
                let Some(index) = extract_numeric_arg(&call.arguments[0]) else {
                    return false;
                };
                let Some(key) = extract_string_arg(&call.arguments[1]) else {
                    return false;
                };
                (info, index as i64, key)
            }
            DecoderEntry::Wrapper { mapping, decoder_info } => {
                // Wrapper call: _0x5d34f5(a, b, c, d, "key")
                if call.arguments.len() <= mapping.index_parameter
                    || call.arguments.len() <= mapping.key_parameter
                {
                    return false;
                }
                let Some(raw_index) =
                    extract_numeric_arg(&call.arguments[mapping.index_parameter])
                else {
                    return false;
                };
                let Some(key) = extract_string_arg(&call.arguments[mapping.key_parameter])
                else {
                    return false;
                };
                let effective = raw_index as i64 + mapping.index_offset;
                (decoder_info, effective, key)
            }
        };

        // Compute the array index.
        let array_index = effective_index - info.base_offset as i64;
        if array_index < 0 || array_index as usize >= info.array.len() {
            return false;
        }

        let encoded = &info.array[array_index as usize];
        let Some(decoded) = crate::utils::base64_rc4_decode(encoded, &key) else {
            return false;
        };

        let value = context.ast.atom(&decoded);
        let replacement = context.ast.expression_string_literal(SPAN, value, None);
        operations::replace_expression(expression, replacement, context);
        true
    }
}

impl StringArrayRC4DecoderTransformer {
    /// Scan a scope for wrapper functions that forward to a known decoder/wrapper.
    fn find_wrappers_in_scope<'a>(
        &self,
        statements: &ArenaVec<'a, Statement<'a>>,
        decoders: &mut HashMap<SymbolId, DecoderEntry>,
        context: &TraverseCtx<'a, ()>,
    ) -> bool {
        let mut new_wrappers: Vec<(SymbolId, DecoderEntry)> = Vec::new();

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
            let Some(wrapper_symbol) = binding.symbol_id.get() else {
                continue;
            };

            if decoders.contains_key(&wrapper_symbol) {
                continue;
            }

            if let Some((target_symbol, mapping)) =
                classify_wrapper_function(function, context)
            {
                // Check if the target is a known decoder or wrapper.
                if let Some(entry) = decoders.get(&target_symbol) {
                    let decoder_info = match entry {
                        DecoderEntry::Decoder(info) => info.clone(),
                        DecoderEntry::Wrapper { decoder_info, mapping: parent_mapping } => {
                            // Compose mappings — for now, only handle direct wrappers.
                            // Chained wrappers would need parameter index remapping.
                            let _ = parent_mapping;
                            decoder_info.clone()
                        }
                    };
                    new_wrappers.push((
                        wrapper_symbol,
                        DecoderEntry::Wrapper {
                            mapping,
                            decoder_info,
                        },
                    ));
                }
            }
        }

        let found_any = !new_wrappers.is_empty();
        for (symbol, entry) in new_wrappers {
            decoders.insert(symbol, entry);
        }
        found_any
    }
}

/// Extract a numeric value from a call argument.
fn extract_numeric_arg(argument: &oxc_ast::ast::Argument<'_>) -> Option<f64> {
    let expression = argument.as_expression()?;
    match expression {
        Expression::NumericLiteral(number) => Some(number.value),
        Expression::UnaryExpression(unary)
            if unary.operator == oxc_syntax::operator::UnaryOperator::UnaryNegation =>
        {
            if let Expression::NumericLiteral(number) = &unary.argument {
                Some(-number.value)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Extract a string value from a call argument.
fn extract_string_arg(argument: &oxc_ast::ast::Argument<'_>) -> Option<String> {
    let expression = argument.as_expression()?;
    if let Expression::StringLiteral(string) = expression {
        Some(string.value.to_string())
    } else {
        None
    }
}

/// Extract the string array from a self-replacing function.
fn extract_self_replacing_array(function: &oxc_ast::ast::Function<'_>) -> Option<Vec<String>> {
    let body = function.body.as_ref()?;
    if !function.params.items.is_empty() {
        return None;
    }

    for statement in &body.statements {
        if let Statement::VariableDeclaration(declaration) = statement {
            for declarator in &declaration.declarations {
                if let Some(Expression::ArrayExpression(array)) = &declarator.init {
                    let mut elements = Vec::new();
                    let mut all_strings = true;
                    for element in &array.elements {
                        if let Some(Expression::StringLiteral(literal)) = element.as_expression() {
                            elements.push(literal.value.to_string());
                        } else {
                            all_strings = false;
                            break;
                        }
                    }
                    if all_strings && elements.len() >= 10 {
                        return Some(elements);
                    }
                }
            }
        }
    }
    None
}

/// Check if a function is an RC4 decoder: 2 params, subtracts an offset,
/// calls an array function, and contains base64/RC4 decryption logic.
fn classify_rc4_decoder(
    function: &oxc_ast::ast::Function<'_>,
    array_functions: &[(SymbolId, Vec<String>)],
    context: &TraverseCtx<'_, ()>,
) -> Option<(usize, usize)> {
    let body = function.body.as_ref()?;

    let mut analyzer = RC4DecoderAnalyzer::new();
    analyzer.visit_function_body(body);

    // Must have a subtraction offset.
    let offset = analyzer.subtraction_offset? as usize;

    // Must reference one of the array functions.
    let array_func_idx = array_functions.iter().position(|&(sym, _)| {
        analyzer.called_symbols.contains(&context.scoping().symbol_name(sym).to_string())
    })?;

    // Must contain the standard base64 alphabet (indicator of base64+RC4 pattern).
    if !analyzer.has_base64_alphabet {
        return None;
    }

    Some((array_func_idx, offset))
}

/// Classify a function as a wrapper that forwards to a decoder.
///
/// Pattern: `function f(a, b, c, d, e) { return _0x1a5c(paramN +/- OFFSET, paramM); }`
fn classify_wrapper_function(
    function: &oxc_ast::ast::Function<'_>,
    context: &TraverseCtx<'_, ()>,
) -> Option<(SymbolId, WrapperMapping)> {
    let body = function.body.as_ref()?;

    // Must have a single return statement.
    if body.statements.len() != 1 {
        return None;
    }
    let Statement::ReturnStatement(return_statement) = &body.statements[0] else {
        return None;
    };
    let Some(argument) = &return_statement.argument else {
        return None;
    };

    // The return expression must be a call with 2 arguments.
    let Expression::CallExpression(call) = argument else {
        return None;
    };
    if call.arguments.len() != 2 {
        return None;
    }

    // Callee must be an identifier.
    let Expression::Identifier(callee) = &call.callee else {
        return None;
    };
    let Some(ref_id) = callee.reference_id.get() else {
        return None;
    };
    let reference = context.scoping().get_reference(ref_id);
    let target_symbol = reference.symbol_id()?;

    // Collect the wrapper's parameter names.
    let param_names: Vec<String> = function
        .params
        .items
        .iter()
        .filter_map(|param| {
            if let oxc_ast::ast::BindingPattern::BindingIdentifier(binding) = &param.pattern {
                Some(binding.name.to_string())
            } else {
                None
            }
        })
        .collect();

    if param_names.is_empty() {
        return None;
    }

    // First argument: paramN +/- OFFSET (the index computation).
    let (index_parameter, index_offset) =
        extract_parameter_with_offset(&call.arguments[0], &param_names)?;

    // Second argument: paramM (the RC4 key).
    let key_parameter = extract_parameter_index(&call.arguments[1], &param_names)?;

    Some((
        target_symbol,
        WrapperMapping {
            index_parameter,
            index_offset,
            key_parameter,
        },
    ))
}

/// Extract a parameter reference with an optional arithmetic offset.
///
/// Matches: `param`, `param - OFFSET`, `param + OFFSET`, `param - -OFFSET`.
fn extract_parameter_with_offset(
    argument: &oxc_ast::ast::Argument<'_>,
    param_names: &[String],
) -> Option<(usize, i64)> {
    let expression = argument.as_expression()?;

    // Simple parameter reference (no offset).
    if let Expression::Identifier(id) = expression {
        let idx = param_names.iter().position(|n| n == id.name.as_str())?;
        return Some((idx, 0));
    }

    // param +/- OFFSET
    if let Expression::BinaryExpression(binary) = expression {
        let Expression::Identifier(id) = &binary.left else {
            return None;
        };
        let idx = param_names.iter().position(|n| n == id.name.as_str())?;

        match binary.operator {
            oxc_syntax::operator::BinaryOperator::Subtraction => {
                // param - OFFSET or param - -OFFSET
                if let Expression::NumericLiteral(lit) = &binary.right {
                    return Some((idx, -(lit.value as i64)));
                }
                if let Expression::UnaryExpression(unary) = &binary.right {
                    if unary.operator == oxc_syntax::operator::UnaryOperator::UnaryNegation {
                        if let Expression::NumericLiteral(lit) = &unary.argument {
                            return Some((idx, lit.value as i64)); // param - -N = param + N
                        }
                    }
                }
            }
            oxc_syntax::operator::BinaryOperator::Addition => {
                if let Expression::NumericLiteral(lit) = &binary.right {
                    return Some((idx, lit.value as i64));
                }
            }
            _ => {}
        }
    }

    None
}

/// Extract a simple parameter reference index.
fn extract_parameter_index(
    argument: &oxc_ast::ast::Argument<'_>,
    param_names: &[String],
) -> Option<usize> {
    let expression = argument.as_expression()?;
    if let Expression::Identifier(id) = expression {
        param_names.iter().position(|n| n == id.name.as_str())
    } else {
        None
    }
}

/// Analyzer that checks a decoder function body for RC4 indicators.
struct RC4DecoderAnalyzer {
    subtraction_offset: Option<f64>,
    called_symbols: Vec<String>,
    has_base64_alphabet: bool,
}

impl RC4DecoderAnalyzer {
    fn new() -> Self {
        Self {
            subtraction_offset: None,
            called_symbols: Vec::new(),
            has_base64_alphabet: false,
        }
    }
}

impl<'a> Visit<'a> for RC4DecoderAnalyzer {
    fn visit_assignment_expression(
        &mut self,
        assignment: &oxc_ast::ast::AssignmentExpression<'a>,
    ) {
        if assignment.operator == oxc_syntax::operator::AssignmentOperator::Assign {
            if let Expression::BinaryExpression(binary) = &assignment.right {
                if binary.operator == oxc_syntax::operator::BinaryOperator::Subtraction {
                    if self.subtraction_offset.is_none() {
                        // Try a direct numeric literal first.
                        if let Expression::NumericLiteral(literal) = &binary.right {
                            self.subtraction_offset = Some(literal.value);
                        } else {
                            // Try evaluating a compound expression (e.g., `0x1379 + -0x13c6 + 0x207`).
                            if let Some(value) =
                                crate::utils::eval::try_eval(&binary.right)
                                    .and_then(|v| v.as_number())
                            {
                                self.subtraction_offset = Some(value);
                            }
                        }
                    }
                }
            }
        }
        walk::walk_assignment_expression(self, assignment);
    }

    fn visit_call_expression(&mut self, call: &oxc_ast::ast::CallExpression<'a>) {
        if let Expression::Identifier(id) = &call.callee {
            self.called_symbols.push(id.name.to_string());
        }
        walk::walk_call_expression(self, call);
    }

    fn visit_string_literal(&mut self, literal: &oxc_ast::ast::StringLiteral<'a>) {
        let value = literal.value.as_str();
        if value.contains("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789+/") {
            self.has_base64_alphabet = true;
        }
        walk::walk_string_literal(self, literal);
    }
}

// ---------------------------------------------------------------------------
// Rotation IIFE detection and solving
// ---------------------------------------------------------------------------

/// Extract the rotation target from a rotation IIFE statement.
fn extract_rotation_iife<'a>(
    statement: &Statement<'a>,
    array_function_symbol: SymbolId,
    context: &TraverseCtx<'a, ()>,
) -> Option<i64> {
    let Statement::ExpressionStatement(expr_stmt) = statement else {
        return None;
    };
    let expression = unwrap_parens(&expr_stmt.expression);

    // The IIFE may be a direct call or part of a sequence expression
    // (comma-joined with other IIFEs in minified code).
    if let Expression::SequenceExpression(sequence) = expression {
        for sub_expression in &sequence.expressions {
            if let Some(target) =
                try_extract_iife_call(unwrap_parens(sub_expression), array_function_symbol, context)
            {
                return Some(target);
            }
        }
        return None;
    }

    try_extract_iife_call(expression, array_function_symbol, context)
}

/// Try to match a single expression as a rotation IIFE call.
fn try_extract_iife_call<'a>(
    expression: &Expression<'a>,
    array_function_symbol: SymbolId,
    context: &TraverseCtx<'a, ()>,
) -> Option<i64> {
    let Expression::CallExpression(call) = expression else {
        return None;
    };
    let callee = unwrap_parens(&call.callee);
    if !matches!(callee, Expression::FunctionExpression(_)) {
        return None;
    }
    if call.arguments.len() != 2 {
        return None;
    }

    // First arg must reference the array function.
    let first_arg = call.arguments[0].as_expression()?;
    let Expression::Identifier(id) = unwrap_parens(first_arg) else {
        return None;
    };
    let ref_id = id.reference_id.get()?;
    let reference = context.scoping().get_reference(ref_id);
    if reference.symbol_id() != Some(array_function_symbol) {
        return None;
    }

    // Second arg must evaluate to a numeric target.
    let second_arg = call.arguments[1].as_expression()?;
    let target_expression = unwrap_parens(second_arg);
    let target_value = if let Expression::NumericLiteral(target) = target_expression {
        target.value as i64
    } else {
        crate::utils::eval::try_eval(target_expression)?.as_number()? as i64
    };
    Some(target_value)
}

/// Solve the array rotation by trying each rotation and evaluating the checksum.
fn solve_rotation_rc4(
    array: &[String],
    base_offset: usize,
    target: i64,
    iife_statement: &Statement<'_>,
    decoder_symbol: SymbolId,
    context: &TraverseCtx<'_, ()>,
) -> Option<Vec<String>> {
    let mut rotated = array.to_vec();
    let array_len = rotated.len();

    if array_len == 0 {
        return None;
    }

    // Extract wrapper functions from inside the IIFE for checksum evaluation.
    let wrappers = extract_iife_wrappers(iife_statement, decoder_symbol, context);

    for rotation in 0..array_len {
        let checksum = eval_rc4_checksum(iife_statement, &rotated, base_offset, &wrappers);
        if rotation < 3 || rotation == 309 || checksum.is_some() {
        }
        if let Some(checksum) = checksum {
            if checksum == target {
                return Some(rotated);
            }
        }

        // Rotate left by 1.
        rotated.rotate_left(1);
    }

    None
}

/// A wrapper function inside the rotation IIFE.
#[derive(Debug)]
struct IIFEWrapper {
    name: String,
    mapping: WrapperMapping,
}

/// Extract wrapper function definitions from inside the rotation IIFE.
fn extract_iife_wrappers(
    iife_statement: &Statement<'_>,
    decoder_symbol: SymbolId,
    context: &TraverseCtx<'_, ()>,
) -> Vec<IIFEWrapper> {
    let mut wrappers = Vec::new();

    let Statement::ExpressionStatement(expr_stmt) = iife_statement else {
        return wrappers;
    };
    let expression = unwrap_parens(&expr_stmt.expression);

    // The IIFE call may be inside a SequenceExpression.
    let iife_calls: Vec<&Expression<'_>> = if let Expression::SequenceExpression(sequence) = expression {
        sequence
            .expressions
            .iter()
            .map(|e| unwrap_parens(e))
            .collect()
    } else {
        vec![expression]
    };

    for call_expression in iife_calls {
        let Expression::CallExpression(call) = call_expression else {
            continue;
        };
        let callee = unwrap_parens(&call.callee);
        let Expression::FunctionExpression(func) = callee else {
            continue;
        };
        let Some(body) = &func.body else {
            continue;
        };

        for statement in &body.statements {
            let Statement::FunctionDeclaration(function) = statement else {
                continue;
            };
            let Some(binding) = &function.id else {
                continue;
            };
            let name = binding.name.to_string();

            if let Some((target_sym, mapping)) = classify_wrapper_function(function, context) {
                if target_sym == decoder_symbol {
                    wrappers.push(IIFEWrapper { name, mapping });
                }
            }
        }
    }

    wrappers
}

/// Evaluate the checksum expression inside the rotation IIFE.
fn eval_rc4_checksum(
    iife_statement: &Statement<'_>,
    array: &[String],
    base_offset: usize,
    wrappers: &[IIFEWrapper],
) -> Option<i64> {
    let Statement::ExpressionStatement(expr_stmt) = iife_statement else {
        return None;
    };
    let expression = unwrap_parens(&expr_stmt.expression);

    // The IIFE call may be inside a SequenceExpression.
    let calls: Vec<&Expression<'_>> = if let Expression::SequenceExpression(sequence) = expression {
        sequence.expressions.iter().map(|e| unwrap_parens(e)).collect()
    } else {
        vec![expression]
    };

    for call_expression in calls {
        let Expression::CallExpression(call) = call_expression else {
            continue;
        };
        let callee = unwrap_parens(&call.callee);
        let Expression::FunctionExpression(func) = callee else {
            continue;
        };
        let Some(body) = &func.body else {
            continue;
        };

        if let Some(result) = find_checksum_in_body(&body.statements, array, base_offset, wrappers) {
            return Some(result);
        }
    }

    None
}

fn find_checksum_in_body(
    statements: &[Statement<'_>],
    array: &[String],
    base_offset: usize,
    wrappers: &[IIFEWrapper],
) -> Option<i64> {
    for statement in statements {
        match statement {
            Statement::VariableDeclaration(declaration) => {
                for declarator in &declaration.declarations {
                    if let Some(init) = &declarator.init {
                        let result = eval_rc4_expression(init, array, base_offset, wrappers);
                        if result.is_some() {
                            return Some(result.unwrap() as i64);
                        }
                    }
                }
            }
            Statement::WhileStatement(while_stmt) => {
                if let Statement::BlockStatement(block) = &while_stmt.body {
                    if let Some(result) =
                        find_checksum_in_body(&block.body, array, base_offset, wrappers)
                    {
                        return Some(result);
                    }
                }
            }
            Statement::TryStatement(try_stmt) => {
                if let Some(result) =
                    find_checksum_in_body(&try_stmt.block.body, array, base_offset, wrappers)
                {
                    return Some(result);
                }
            }
            Statement::BlockStatement(block) => {
                if let Some(result) =
                    find_checksum_in_body(&block.body, array, base_offset, wrappers)
                {
                    return Some(result);
                }
            }
            _ => {}
        }
    }
    None
}

/// Recursively evaluate a checksum expression, handling wrapper calls via RC4.
fn eval_rc4_expression(
    expression: &Expression<'_>,
    array: &[String],
    base_offset: usize,
    wrappers: &[IIFEWrapper],
) -> Option<f64> {
    let expression = unwrap_parens(expression);

    // parseInt(wrapper(...)) — resolve the wrapper call via RC4 decode.
    if let Expression::CallExpression(call) = expression {
        if let Expression::Identifier(callee_id) = &call.callee {
            if callee_id.name.as_str() == "parseInt" && !call.arguments.is_empty() {
                if let Some(arg) = call.arguments[0].as_expression() {
                    let arg = unwrap_parens(arg);

                    if let Expression::CallExpression(inner_call) = arg {
                        if let Expression::Identifier(inner_callee) = &inner_call.callee {
                            // Try to resolve via wrapper functions.
                            let wrapper = wrappers
                                .iter()
                                .find(|w| w.name == inner_callee.name.as_str());

                            if let Some(wrapper) = wrapper {
                                let raw_index = extract_numeric_arg(
                                    &inner_call.arguments[wrapper.mapping.index_parameter],
                                )?;
                                let key = extract_string_arg(
                                    &inner_call.arguments[wrapper.mapping.key_parameter],
                                )?;
                                let effective =
                                    raw_index as i64 + wrapper.mapping.index_offset;
                                let array_index =
                                    (effective - base_offset as i64) as usize;
                                if array_index >= array.len() {
                                    return None;
                                }
                                let encoded = &array[array_index];
                                let decoded =
                                    crate::utils::base64_rc4_decode(encoded, &key)?;
                                return crate::utils::eval::js_parse_int(&decoded, None);
                            }
                        }
                    }
                }
            }
        }
    }

    // Unary expressions.
    if let Expression::UnaryExpression(unary) = expression {
        let argument = eval_rc4_expression(&unary.argument, array, base_offset, wrappers)?;
        return match unary.operator {
            oxc_syntax::operator::UnaryOperator::UnaryNegation => Some(-argument),
            oxc_syntax::operator::UnaryOperator::UnaryPlus => Some(argument),
            _ => None,
        };
    }

    // Binary expressions.
    if let Expression::BinaryExpression(binary) = expression {
        let left = eval_rc4_expression(&binary.left, array, base_offset, wrappers)?;
        let right = eval_rc4_expression(&binary.right, array, base_offset, wrappers)?;
        return match binary.operator {
            oxc_syntax::operator::BinaryOperator::Addition => Some(left + right),
            oxc_syntax::operator::BinaryOperator::Subtraction => Some(left - right),
            oxc_syntax::operator::BinaryOperator::Multiplication => Some(left * right),
            oxc_syntax::operator::BinaryOperator::Division if right != 0.0 => Some(left / right),
            oxc_syntax::operator::BinaryOperator::Remainder if right != 0.0 => {
                Some(left % right)
            }
            _ => None,
        };
    }

    // Leaf values — delegate to the shared evaluator.
    crate::utils::eval::try_eval(expression)?.as_number()
}

