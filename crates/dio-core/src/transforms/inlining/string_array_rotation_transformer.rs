//! Decodes and inlines string array lookups that use rotation and index offset.
//!
//! This handles the obfuscator.io pattern where:
//! 1. A self-replacing function returns an array of plain-text strings
//! 2. An IIFE rotates the array (`push(shift())`) until a checksum matches
//! 3. A decoder function subtracts an offset before indexing
//!
//! # Example
//!
//! ```js
//! function _0x271f() {
//!     var arr = ['log', 'error', 'console', ...];
//!     _0x271f = function() { return arr; };
//!     return _0x271f();
//! }
//! // Rotation IIFE shuffles the array
//! (function(getArr, target) {
//!     var arr = getArr();
//!     while (true) {
//!         try {
//!             var checksum = -parseInt(decoder(0xa1)) / 1 + ...;
//!             if (checksum === target) break;
//!             else arr.push(arr.shift());
//!         } catch(e) { arr.push(arr.shift()); }
//!     }
//! }(_0x271f, 0x1affb));
//! function _0x5851(n) {
//!     n = n - 0x99;
//!     return _0x271f()[n];
//! }
//! // Usage: _0x5851(0xad) → "log"
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

/// Information about a detected rotation-based string array.
#[derive(Debug, Clone)]
struct RotationDecoderInfo {
    /// The final array contents after rotation, indexed by the offset-adjusted position.
    /// Key: the raw argument value passed to the decoder (before offset subtraction).
    /// Value: the decoded string.
    decoded_values: HashMap<usize, String>,
}

/// Decodes rotation-based string array lookups.
pub struct StringArrayRotationTransformer {
    /// Maps decoder function SymbolId -> decoded values.
    decoders: Mutex<HashMap<SymbolId, RotationDecoderInfo>>,
}

impl Default for StringArrayRotationTransformer {
    fn default() -> Self {
        Self {
            decoders: Mutex::new(HashMap::new()),
        }
    }
}

impl StringArrayRotationTransformer {
    /// Try to find and decode a rotation-based string array pattern in a statement list.
    ///
    /// Looks for:
    /// 1. A self-replacing function that returns an array of strings
    /// 2. A decoder function that subtracts an offset and indexes the array
    /// 3. A rotation IIFE that shuffles the array
    fn find_rotation_pattern<'a>(
        statements: &ArenaVec<'a, Statement<'a>>,
        context: &TraverseCtx<'a, ()>,
    ) -> Option<RotationPatternMatch> {
        // Step 1: Find self-replacing array functions.
        // Pattern: function _0x271f() { var arr = [...]; _0x271f = function() { return arr; }; return _0x271f(); }
        let mut array_functions: Vec<ArrayFunctionInfo> = Vec::new();

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

            if let Some(elements) = Self::extract_self_replacing_array(function) {
                array_functions.push(ArrayFunctionInfo {
                    symbol_id,
                    elements,
                });
            }
        }


        if array_functions.is_empty() {
            return None;
        }

        // Step 2: Find decoder functions that reference one of these array functions.
        // Pattern: function _0x5851(n) { n = n - OFFSET; var arr = _0x271f(); return arr[n]; }
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
            let Some(decoder_symbol_id) = binding.symbol_id.get() else {
                continue;
            };

            if let Some((array_func_index, offset)) =
                Self::classify_offset_decoder(function, &array_functions, context)
            {
                let array_func_info = &array_functions[array_func_index];

                // Step 3: Find the rotation IIFE.
                // Pattern: (function(getArr, target) { ... arr.push(arr.shift()) ... }(_0x271f, TARGET))
                for iife_statement in statements.iter() {
                    if let Some(target) = Self::extract_rotation_iife(
                        iife_statement,
                        array_func_info.symbol_id,
                        context,
                    ) {
                        // Step 4: Simulate the rotation.
                        let mut array = array_func_info.elements.clone();
                        let array_len = array.len();

                        if array_len == 0 {
                            continue;
                        }

                        // Try up to array_len rotations (it must converge within one full cycle).
                        let mut found = false;
                        for _ in 0..array_len {
                            let checksum = Self::compute_checksum(
                                &array,
                                offset,
                                iife_statement,
                            );

                            if let Some(checksum) = checksum {
                                if checksum == target {
                                    found = true;
                                    break;
                                }
                            }

                            // Rotate left by 1: push(shift())
                            let first = array.remove(0);
                            array.push(first);
                        }

                        if !found {
                            continue;
                        }

                        // Build the decoded values map.
                        let mut decoded_values = HashMap::new();
                        for (i, value) in array.iter().enumerate() {
                            // The raw argument to the decoder is i + offset
                            decoded_values.insert(i + offset, value.clone());
                        }

                        return Some(RotationPatternMatch {
                            decoder_symbol_id,
                            decoded_values,
                        });
                    }
                }
            }
        }

        None
    }

    /// Extract the string array from a self-replacing function.
    fn extract_self_replacing_array(
        function: &oxc_ast::ast::Function<'_>,
    ) -> Option<Vec<String>> {
        let body = function.body.as_ref()?;

        // Must have no parameters.
        if !function.params.items.is_empty() {
            return None;
        }

        // Look for a variable declaration with an array expression.
        let mut found_array: Option<Vec<String>> = None;

        for statement in &body.statements {
            if let Statement::VariableDeclaration(declaration) = statement {
                for declarator in &declaration.declarations {
                    if let Some(init) = &declarator.init {
                        if let Expression::ArrayExpression(array) = init {
                            let mut elements = Vec::new();
                            let mut all_strings = true;

                            for element in &array.elements {
                                if let Some(expr) = element.as_expression() {
                                    if let Expression::StringLiteral(literal) = expr {
                                        elements.push(literal.value.to_string());
                                    } else {
                                        all_strings = false;
                                        break;
                                    }
                                } else {
                                    all_strings = false;
                                    break;
                                }
                            }

                            if all_strings && elements.len() >= 3 {
                                found_array = Some(elements);
                            }
                        }
                    }
                }
            }
        }

        found_array
    }

    /// Check if a function is an offset decoder and return the matching array
    /// function index and the offset value.
    fn classify_offset_decoder<'a>(
        function: &oxc_ast::ast::Function<'a>,
        array_functions: &[ArrayFunctionInfo],
        context: &TraverseCtx<'a, ()>,
    ) -> Option<(usize, usize)> {
        let body = function.body.as_ref()?;

        // Must have at least 1 parameter.
        if function.params.items.is_empty() {
            return None;
        }

        // Look for the offset subtraction pattern and array function call.
        let mut analyzer = OffsetDecoderAnalyzer::new();
        analyzer.visit_function_body(body);

        // Must have a subtraction offset.
        let offset = analyzer.subtraction_offset? as usize;

        // Must call one of the array functions.
        let array_func_index = array_functions.iter().position(|af| {
            analyzer.called_function_names.iter().any(|name| {
                context.scoping().symbol_name(af.symbol_id) == name.as_str()
            })
        })?;

        Some((array_func_index, offset))
    }

    /// Check if a statement is a rotation IIFE that targets the given array function.
    fn extract_rotation_iife<'a>(
        statement: &Statement<'a>,
        array_function_symbol_id: SymbolId,
        context: &TraverseCtx<'a, ()>,
    ) -> Option<i64> {
        // Pattern: ExpressionStatement containing a CallExpression
        // where the callee is a function expression and the first argument
        // references the array function.
        let Statement::ExpressionStatement(expr_stmt) = statement else {
            return None;
        };

        let expression = unwrap_parens(&expr_stmt.expression);

        // The IIFE can be: `(function(a,b){...})(arg1, arg2)` which parses as
        // CallExpression, or the whole thing could be wrapped in another
        // SequenceExpression or similar. Handle CallExpression directly.
        let Expression::CallExpression(call) = expression else {
            return None;
        };

        let callee = unwrap_parens(&call.callee);

        // Callee must be a function expression.
        if !matches!(callee, Expression::FunctionExpression(_)) {
            return None;
        }

        // Must have 2 arguments: the array function and the target number.
        if call.arguments.len() != 2 {
            return None;
        }

        // First argument must reference the array function.
        let Some(first_arg) = call.arguments[0].as_expression() else {
            return None;
        };
        let Expression::Identifier(id) = unwrap_parens(first_arg) else {
            return None;
        };
        let Some(ref_id) = id.reference_id.get() else {
            return None;
        };
        let reference = context.scoping().get_reference(ref_id);
        if reference.symbol_id() != Some(array_function_symbol_id) {
            return None;
        }

        // Second argument must be a numeric literal (the target checksum).
        let Some(second_arg) = call.arguments[1].as_expression() else {
            return None;
        };
        let Expression::NumericLiteral(target) = unwrap_parens(second_arg) else {
            return None;
        };

        Some(target.value as i64)
    }

    /// Compute the checksum for the current array state by finding and
    /// evaluating the checksum expression in the rotation IIFE.
    fn compute_checksum(
        array: &[String],
        offset: usize,
        iife_statement: &Statement<'_>,
    ) -> Option<i64> {
        Self::find_and_eval_checksum(iife_statement, array, offset)
    }

    /// Find and evaluate the checksum expression inside the rotation IIFE
    /// for the given array state.
    fn find_and_eval_checksum(
        iife_statement: &Statement<'_>,
        array: &[String],
        offset: usize,
    ) -> Option<i64> {
        let Statement::ExpressionStatement(expr_stmt) = iife_statement else {
            return None;
        };
        let expression = unwrap_parens(&expr_stmt.expression);
        let Expression::CallExpression(call) = expression else {
            return None;
        };
        let callee = unwrap_parens(&call.callee);
        let Expression::FunctionExpression(func) = callee else {
            return None;
        };
        let body = func.body.as_ref()?;

        // Walk all variable declarations in the IIFE body looking for one
        // whose initializer contains parseInt.
        find_and_eval_checksum_in_body(&body.statements, array, offset)
    }
}


impl StringArrayRotationTransformer {
    /// Scan a statement list for variable declarations like `var alias = decoder;`
    /// and register them in the decoders map with the same decoded values.
    fn find_aliases<'a>(
        statements: &ArenaVec<'a, Statement<'a>>,
        decoder_symbol_id: SymbolId,
        _decoder_name: &str,
        decoders: &mut HashMap<SymbolId, RotationDecoderInfo>,
        context: &TraverseCtx<'a, ()>,
    ) {
        for statement in statements.iter() {
            let Statement::VariableDeclaration(declaration) = statement else {
                continue;
            };
            for declarator in &declaration.declarations {
                let oxc_ast::ast::BindingPattern::BindingIdentifier(binding) = &declarator.id
                else {
                    continue;
                };
                let Some(alias_symbol_id) = binding.symbol_id.get() else {
                    continue;
                };

                // Already registered?
                if decoders.contains_key(&alias_symbol_id) {
                    continue;
                }

                // Check if the initializer is a reference to the decoder function.
                let Some(init) = &declarator.init else {
                    continue;
                };
                let init = unwrap_parens(init);
                let Expression::Identifier(id) = init else {
                    continue;
                };
                let Some(ref_id) = id.reference_id.get() else {
                    continue;
                };
                let reference = context.scoping().get_reference(ref_id);
                let Some(init_symbol_id) = reference.symbol_id() else {
                    continue;
                };

                if init_symbol_id == decoder_symbol_id {
                    if let Some(decoder_info) = decoders.get(&decoder_symbol_id) {
                        let cloned = decoder_info.clone();
                        decoders.insert(alias_symbol_id, cloned);
                    }
                }
            }
        }
    }
}

/// Information about a self-replacing array function.
struct ArrayFunctionInfo {
    symbol_id: SymbolId,
    elements: Vec<String>,
}

/// Match result for the full rotation pattern.
struct RotationPatternMatch {
    decoder_symbol_id: SymbolId,
    decoded_values: HashMap<usize, String>,
}

/// Analyzes a decoder function body to find the subtraction offset.
struct OffsetDecoderAnalyzer {
    subtraction_offset: Option<f64>,
    called_function_names: Vec<String>,
}

impl OffsetDecoderAnalyzer {
    fn new() -> Self {
        Self {
            subtraction_offset: None,
            called_function_names: Vec::new(),
        }
    }
}

impl<'a> Visit<'a> for OffsetDecoderAnalyzer {
    fn visit_assignment_expression(
        &mut self,
        assignment: &oxc_ast::ast::AssignmentExpression<'a>,
    ) {
        // Look for `param = param - OFFSET`
        if assignment.operator == oxc_syntax::operator::AssignmentOperator::Assign {
            if let Expression::BinaryExpression(binary) = &assignment.right {
                if binary.operator == oxc_syntax::operator::BinaryOperator::Subtraction {
                    if let Expression::NumericLiteral(literal) = &binary.right {
                        self.subtraction_offset = Some(literal.value);
                    }
                }
            }
        }
        walk::walk_assignment_expression(self, assignment);
    }

    fn visit_call_expression(&mut self, call: &oxc_ast::ast::CallExpression<'a>) {
        if let Expression::Identifier(id) = &call.callee {
            self.called_function_names.push(id.name.to_string());
        }
        walk::walk_call_expression(self, call);
    }
}

/// Recursively search statement lists (including nested while/try/catch blocks)
/// for a variable declarator whose initializer evaluates successfully as a checksum.
fn find_and_eval_checksum_in_body(
    statements: &[Statement<'_>],
    array: &[String],
    offset: usize,
) -> Option<i64> {
    for statement in statements {
        match statement {
            Statement::VariableDeclaration(declaration) => {
                for declarator in &declaration.declarations {
                    if let Some(init) = &declarator.init {
                        if let Some(result) = eval_checksum_expression(init, array, offset) {
                            return Some(result as i64);
                        }
                    }
                }
            }
            Statement::WhileStatement(while_stmt) => {
                if let Statement::BlockStatement(block) = &while_stmt.body {
                    if let Some(result) =
                        find_and_eval_checksum_in_body(&block.body, array, offset)
                    {
                        return Some(result);
                    }
                }
            }
            Statement::TryStatement(try_stmt) => {
                if let Some(result) =
                    find_and_eval_checksum_in_body(&try_stmt.block.body, array, offset)
                {
                    return Some(result);
                }
            }
            Statement::BlockStatement(block) => {
                if let Some(result) =
                    find_and_eval_checksum_in_body(&block.body, array, offset)
                {
                    return Some(result);
                }
            }
            _ => {}
        }
    }
    None
}

/// Recursively evaluate a checksum expression given the current array state.
///
/// Handles: numeric literals, unary negation, binary +/-/*, division,
/// `parseInt(decoder(HEX))` calls, and parenthesized expressions.
fn eval_checksum_expression(
    expression: &Expression<'_>,
    array: &[String],
    offset: usize,
) -> Option<f64> {
    let expression = unwrap_parens(expression);

    match expression {
        Expression::NumericLiteral(literal) => Some(literal.value),

        Expression::UnaryExpression(unary) => {
            let argument = eval_checksum_expression(&unary.argument, array, offset)?;
            match unary.operator {
                oxc_syntax::operator::UnaryOperator::UnaryNegation => Some(-argument),
                oxc_syntax::operator::UnaryOperator::UnaryPlus => Some(argument),
                _ => None,
            }
        }

        Expression::BinaryExpression(binary) => {
            let left = eval_checksum_expression(&binary.left, array, offset)?;
            let right = eval_checksum_expression(&binary.right, array, offset)?;
            match binary.operator {
                oxc_syntax::operator::BinaryOperator::Addition => Some(left + right),
                oxc_syntax::operator::BinaryOperator::Subtraction => Some(left - right),
                oxc_syntax::operator::BinaryOperator::Multiplication => Some(left * right),
                oxc_syntax::operator::BinaryOperator::Division => {
                    if right == 0.0 {
                        None
                    } else {
                        Some(left / right)
                    }
                }
                oxc_syntax::operator::BinaryOperator::Remainder => {
                    if right == 0.0 {
                        None
                    } else {
                        Some(left % right)
                    }
                }
                _ => None,
            }
        }

        Expression::CallExpression(call) => {
            let callee = unwrap_parens(&call.callee);

            // parseInt(decoder(HEX)) or parseInt(decoder(HEX), radix)
            if let Expression::Identifier(id) = callee {
                if id.name.as_str() == "parseInt" && !call.arguments.is_empty() {
                    let Some(arg) = call.arguments[0].as_expression() else {
                        return None;
                    };
                    let arg = unwrap_parens(arg);

                    // The argument is a call to the decoder: decoder(HEX)
                    if let Expression::CallExpression(inner_call) = arg {
                        if inner_call.arguments.len() == 1 {
                            if let Some(inner_arg) = inner_call.arguments[0].as_expression() {
                                if let Expression::NumericLiteral(literal) =
                                    unwrap_parens(inner_arg)
                                {
                                    let raw_index = literal.value as usize;
                                    let array_index = raw_index.checked_sub(offset)?;
                                    if array_index >= array.len() {
                                        return None;
                                    }
                                    let element = &array[array_index];
                                    let parsed = js_parse_int(element)?;
                                    return Some(parsed as f64);
                                }
                            }
                        }
                    }

                    // Could also be parseInt of a string literal directly
                    if let Expression::StringLiteral(literal) = arg {
                        return js_parse_int(literal.value.as_str()).map(|v| v as f64);
                    }
                }
            }

            None
        }

        _ => None,
    }
}

/// Simulate JavaScript's `parseInt` on a string: extract leading digits.
fn js_parse_int(string: &str) -> Option<i64> {
    let mut chars = string.chars().peekable();

    // Skip leading whitespace.
    while chars.peek().map_or(false, |c| c.is_whitespace()) {
        chars.next();
    }

    // Check for sign.
    let negative = match chars.peek() {
        Some('-') => {
            chars.next();
            true
        }
        Some('+') => {
            chars.next();
            false
        }
        _ => false,
    };

    // Collect leading digits.
    let digits: String = chars.take_while(|c| c.is_ascii_digit()).collect();

    if digits.is_empty() {
        return None;
    }

    let value: i64 = digits.parse().ok()?;
    Some(if negative { -value } else { value })
}

/// Unwrap parenthesized expressions.
fn unwrap_parens<'a, 'b>(expression: &'b Expression<'a>) -> &'b Expression<'a> {
    let mut current = expression;
    while let Expression::ParenthesizedExpression(paren) = current {
        current = &paren.expression;
    }
    current
}

impl Transformer for StringArrayRotationTransformer {
    fn name(&self) -> &str {
        "StringArrayRotationTransformer"
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
        if let Some(pattern_match) = Self::find_rotation_pattern(statements, context) {
            let mut decoders = self.decoders.lock().unwrap();

            // Also find aliases: `var alias = decoderFunction;`
            // These are common in obfuscated code where each scope creates
            // a local alias for the decoder.
            let decoder_symbol_id = pattern_match.decoder_symbol_id;
            let decoder_name = context.scoping().symbol_name(decoder_symbol_id).to_string();

            decoders.insert(
                decoder_symbol_id,
                RotationDecoderInfo {
                    decoded_values: pattern_match.decoded_values,
                },
            );

            // Scan ALL statement lists (not just this one) for aliases.
            // Since enter_statements is called for each statement list,
            // we scan the current one for `var x = decoderName;` patterns.
            Self::find_aliases(statements, decoder_symbol_id, &decoder_name, &mut decoders, context);

            // Don't remove declarations here — let the call site replacements
            // in enter_expression remove references first, then the unused
            // variable pruner and dead code transformer handle cleanup.
        } else {
            // Even if we don't find a new pattern in this statement list,
            // scan for aliases of already-known decoders.
            let mut decoders = self.decoders.lock().unwrap();
            if !decoders.is_empty() {
                let known: Vec<(SymbolId, String)> = decoders
                    .keys()
                    .map(|&sid| (sid, context.scoping().symbol_name(sid).to_string()))
                    .collect();
                for (decoder_symbol_id, decoder_name) in &known {
                    Self::find_aliases(statements, *decoder_symbol_id, decoder_name, &mut decoders, context);
                }
            }
        }

        false
    }

    fn enter_expression<'a>(
        &self,
        expression: &mut Expression<'a>,
        context: &mut TraverseCtx<'a, ()>,
    ) -> bool {
        let Expression::CallExpression(call) = expression else {
            return false;
        };

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

        let decoders = self.decoders.lock().unwrap();
        let Some(decoder_info) = decoders.get(&symbol_id) else {
            return false;
        };

        // Must have 1 or 2 arguments (some decoders take an unused second param).
        if call.arguments.is_empty() || call.arguments.len() > 2 {
            return false;
        }

        let Some(first_arg) = call.arguments[0].as_expression() else {
            return false;
        };

        let Some(raw_index) = extract_numeric_index(first_arg) else {
            return false;
        };

        let Some(decoded_string) = decoder_info.decoded_values.get(&raw_index) else {
            return false;
        };

        let value = context.ast.atom(decoded_string);
        let replacement = context.ast.expression_string_literal(SPAN, value, None);
        drop(decoders);
        operations::replace_expression(expression, replacement, context);
        true
    }
}

/// Extract a constant integer index from an expression.
fn extract_numeric_index(expression: &Expression<'_>) -> Option<usize> {
    match expression {
        Expression::NumericLiteral(literal) => {
            let value = literal.value;
            if value >= 0.0 && value == (value as usize as f64) {
                Some(value as usize)
            } else {
                None
            }
        }
        Expression::ParenthesizedExpression(paren) => extract_numeric_index(&paren.expression),
        _ => None,
    }
}
