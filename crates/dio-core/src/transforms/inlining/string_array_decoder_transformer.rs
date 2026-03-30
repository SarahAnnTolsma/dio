//! Decodes and inlines string array lookups used by obfuscation tools.
//!
//! Many JavaScript obfuscators store strings in a central array and replace
//! references with calls to a decoder function that indexes into it. This
//! transformer identifies the array, the decoder function, pre-computes all
//! decoded values, and replaces every call site with the decoded literal.
//!
//! # Supported patterns
//!
//! **Pattern 1: atob-based**
//! ```js
//! var w = ["TnVtYmVy", "ZnVuY3Rpb24", ...];
//! function o(n, t) { return t = w[n], atob(t) }
//! // o(0) → "Number"
//! ```
//!
//! **Pattern 2: Custom base64 alphabet with mixed types**
//! ```js
//! var dn = ["u3ge5zPP", -130.34, "lXgklYsVtWaP", ...];
//! function r(n) {
//!     var t = dn[n];
//!     return "string" == typeof t ? customDecode(t) : t
//! }
//! // r(0) → decoded string, r(2) → -130.34
//! ```

use std::collections::HashMap;
use std::sync::Mutex;

use oxc_allocator::Vec as ArenaVec;
use oxc_ast::ast::{Expression, FunctionType, Statement};
use oxc_ast_visit::{Visit, walk};
use oxc_span::SPAN;
use oxc_syntax::number::NumberBase;
use oxc_syntax::symbol::SymbolId;
use oxc_traverse::TraverseCtx;

use crate::operations;
use crate::transformer::{AstNodeType, Transformer, TransformerPhase, TransformerPriority};
use crate::utils;

/// A pre-decoded value from the string array.
#[derive(Debug, Clone)]
enum DecodedValue {
    String(String),
    Number(f64),
    Boolean(bool),
    Null,
}

/// How the decoder function decodes array entries.
#[derive(Debug, Clone)]
enum DecodeStrategy {
    /// Standard base64 via `atob()`.
    StandardBase64,
    /// Custom 64-character base64 alphabet with type passthrough.
    CustomBase64 { alphabet: Vec<u8> },
}

/// Information about a detected string array and its decoder.
#[derive(Debug, Clone)]
struct DecoderInfo {
    /// Pre-decoded values indexed by array position.
    decoded_values: Vec<DecodedValue>,
}

/// Decodes and inlines string array lookups.
pub struct StringArrayDecoderTransformer {
    /// Maps decoder function SymbolId -> pre-decoded values.
    decoders: Mutex<HashMap<SymbolId, DecoderInfo>>,
}

impl Default for StringArrayDecoderTransformer {
    fn default() -> Self {
        Self {
            decoders: Mutex::new(HashMap::new()),
        }
    }
}

/// Stores the raw value of an array element before decoding.
#[derive(Debug, Clone)]
enum RawArrayElement {
    String(String),
    Number(f64),
    Boolean(bool),
    Null,
    /// Non-literal element (function call, identifier, etc.) — skip.
    NonLiteral,
}

impl StringArrayDecoderTransformer {
    /// Extract all elements from an array expression as raw values.
    fn extract_array_elements(expression: &Expression<'_>) -> Option<Vec<RawArrayElement>> {
        let Expression::ArrayExpression(array) = expression else {
            return None;
        };

        let mut elements = Vec::with_capacity(array.elements.len());
        for element in &array.elements {
            match element {
                oxc_ast::ast::ArrayExpressionElement::StringLiteral(literal) => {
                    elements.push(RawArrayElement::String(literal.value.to_string()));
                }
                oxc_ast::ast::ArrayExpressionElement::NumericLiteral(literal) => {
                    elements.push(RawArrayElement::Number(literal.value));
                }
                oxc_ast::ast::ArrayExpressionElement::BooleanLiteral(literal) => {
                    elements.push(RawArrayElement::Boolean(literal.value));
                }
                oxc_ast::ast::ArrayExpressionElement::NullLiteral(_) => {
                    elements.push(RawArrayElement::Null);
                }
                oxc_ast::ast::ArrayExpressionElement::UnaryExpression(unary) => {
                    // Handle negative numbers: `-130.34`
                    if unary.operator == oxc_syntax::operator::UnaryOperator::UnaryNegation {
                        if let Expression::NumericLiteral(literal) = &unary.argument {
                            elements.push(RawArrayElement::Number(-literal.value));
                            continue;
                        }
                    }
                    elements.push(RawArrayElement::NonLiteral);
                }
                _ => {
                    // Function calls, identifiers, etc. — mark as non-literal.
                    elements.push(RawArrayElement::NonLiteral);
                }
            }
        }

        // Must have a reasonable number of elements.
        if elements.len() < 3 {
            return None;
        }

        Some(elements)
    }

    /// Check if a function declaration is a string array decoder and determine its strategy.
    ///
    /// A decoder function must:
    /// 1. Have at least one parameter
    /// 2. Index into a candidate array via a computed member expression (`array[param]`)
    /// 3. Contain an explicit decoding mechanism: either an `atob` call (pattern 1)
    ///    or a custom base64 alphabet string used with `indexOf` (pattern 2)
    ///
    /// Returns the array's SymbolId and the decode strategy if the function matches.
    fn classify_decoder<'a>(
        function: &oxc_ast::ast::Function<'a>,
        candidate_arrays: &HashMap<SymbolId, Vec<RawArrayElement>>,
        context: &TraverseCtx<'a, ()>,
    ) -> Option<(SymbolId, DecodeStrategy)> {
        let body = function.body.as_ref()?;

        // Must have at least 1 parameter.
        if function.params.items.is_empty() {
            return None;
        }

        // Must not be async or generator.
        if function.r#async || function.generator {
            return None;
        }

        // Walk the function body to collect raw references, then resolve
        // symbol IDs using the scoping context.
        let mut analyzer = FunctionBodyAnalyzer::new();
        analyzer.visit_function_body(body);
        let results = analyzer.resolve(context);

        // Find which candidate array is referenced via resolved symbol IDs.
        let mut matched_array_symbol_id = None;
        for &ref_symbol_id in &results.resolved_symbol_ids {
            if candidate_arrays.contains_key(&ref_symbol_id) {
                matched_array_symbol_id = Some(ref_symbol_id);
                break;
            }
        }

        let array_symbol_id = matched_array_symbol_id?;

        // Must index the array via a computed member expression (`array[n]`).
        // This prevents matching functions that merely reference the array
        // without indexing it.
        if !results
            .computed_member_on_symbols
            .iter()
            .any(|&s| s == array_symbol_id)
        {
            return None;
        }

        // Determine decode strategy. We require an explicit decoding mechanism
        // to avoid false positives with functions that merely read from an array.
        if results.has_atob_reference {
            return Some((array_symbol_id, DecodeStrategy::StandardBase64));
        }

        // For custom base64: require BOTH a 64/65-char alphabet string AND
        // indexOf calls (the decoder uses indexOf to look up character positions).
        if let Some(alphabet) = &results.custom_base64_alphabet {
            if results.has_index_of_call {
                return Some((
                    array_symbol_id,
                    DecodeStrategy::CustomBase64 {
                        alphabet: alphabet.as_bytes().to_vec(),
                    },
                ));
            }
        }

        None
    }

    /// Pre-compute all decoded values from the raw array elements.
    fn decode_array(
        elements: &[RawArrayElement],
        strategy: &DecodeStrategy,
    ) -> Vec<DecodedValue> {
        elements
            .iter()
            .map(|element| match element {
                RawArrayElement::String(string) => {
                    let decoded = match strategy {
                        DecodeStrategy::StandardBase64 => utils::base64_decode(string),
                        DecodeStrategy::CustomBase64 { alphabet } => {
                            utils::base64_decode_with_alphabet(string, alphabet)
                        }
                    };
                    match decoded {
                        Some(decoded_string) => DecodedValue::String(decoded_string),
                        None => DecodedValue::String(string.clone()),
                    }
                }
                RawArrayElement::Number(value) => DecodedValue::Number(*value),
                RawArrayElement::Boolean(value) => DecodedValue::Boolean(*value),
                RawArrayElement::Null => DecodedValue::Null,
                RawArrayElement::NonLiteral => {
                    // Can't pre-decode non-literal elements; leave a placeholder.
                    // Call sites referencing these indices won't be replaced.
                    DecodedValue::Null
                }
            })
            .collect()
    }

    /// Build a replacement expression from a decoded value.
    fn build_replacement<'a>(
        value: &DecodedValue,
        context: &mut TraverseCtx<'a, ()>,
    ) -> Expression<'a> {
        match value {
            DecodedValue::String(string) => {
                let value = context.ast.atom(string);
                context.ast.expression_string_literal(SPAN, value, None)
            }
            DecodedValue::Number(number) => {
                if *number < 0.0 {
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
                    let raw = context.ast.atom(&number.to_string());
                    context.ast.expression_numeric_literal(
                        SPAN,
                        *number,
                        Some(raw),
                        NumberBase::Decimal,
                    )
                }
            }
            DecodedValue::Boolean(value) => {
                context.ast.expression_boolean_literal(SPAN, *value)
            }
            DecodedValue::Null => context.ast.expression_null_literal(SPAN),
        }
    }
}

/// Walks a function body to collect signals for decoder classification.
///
/// Tracks identifier references, computed member access patterns, and
/// specific decoding indicators (atob references, indexOf calls, base64 alphabets).
/// Symbol resolution happens after the walk using `resolve()`.
struct FunctionBodyAnalyzer<'a> {
    /// Raw identifier references with their reference IDs for later resolution.
    identifier_references: Vec<(String, Option<oxc_syntax::reference::ReferenceId>)>,
    /// Identifier references that appear as the object of a computed member
    /// expression (e.g., `array[n]` → reference ID of `array`).
    computed_member_object_references: Vec<Option<oxc_syntax::reference::ReferenceId>>,
    /// Whether the function body references the global `atob` identifier.
    has_atob_reference: bool,
    /// Whether the function body contains an `indexOf` method call.
    has_index_of_call: bool,
    /// If a string literal of length 64-65 is found that looks like a base64
    /// alphabet, store it here.
    custom_base64_alphabet: Option<String>,
    _phantom: std::marker::PhantomData<&'a ()>,
}

/// Results of analyzing a function body, with symbol IDs resolved.
struct AnalyzerResults {
    /// Resolved SymbolIds of identifiers referenced in the function body.
    resolved_symbol_ids: Vec<SymbolId>,
    /// SymbolIds that appear as the object of a computed member expression.
    computed_member_on_symbols: Vec<SymbolId>,
    has_atob_reference: bool,
    has_index_of_call: bool,
    custom_base64_alphabet: Option<String>,
}

impl<'a> FunctionBodyAnalyzer<'a> {
    fn new() -> Self {
        Self {
            identifier_references: Vec::new(),
            computed_member_object_references: Vec::new(),
            has_atob_reference: false,
            has_index_of_call: false,
            custom_base64_alphabet: None,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Resolve collected references using scoping context.
    fn resolve(self, context: &TraverseCtx<'_, ()>) -> AnalyzerResults {
        let mut resolved_symbol_ids = Vec::new();
        for (_, reference_id) in &self.identifier_references {
            if let Some(ref_id) = reference_id {
                let reference = context.scoping().get_reference(*ref_id);
                if let Some(symbol_id) = reference.symbol_id() {
                    resolved_symbol_ids.push(symbol_id);
                }
            }
        }

        let mut computed_member_on_symbols = Vec::new();
        for reference_id in &self.computed_member_object_references {
            if let Some(ref_id) = reference_id {
                let reference = context.scoping().get_reference(*ref_id);
                if let Some(symbol_id) = reference.symbol_id() {
                    computed_member_on_symbols.push(symbol_id);
                }
            }
        }

        AnalyzerResults {
            resolved_symbol_ids,
            computed_member_on_symbols,
            has_atob_reference: self.has_atob_reference,
            has_index_of_call: self.has_index_of_call,
            custom_base64_alphabet: self.custom_base64_alphabet,
        }
    }

    /// Check if a string looks like a base64 alphabet.
    fn looks_like_base64_alphabet(string: &str) -> bool {
        let bytes = string.as_bytes();
        // 64 characters (standard) or 65 characters (with padding sentinel)
        if bytes.len() != 64 && bytes.len() != 65 {
            return false;
        }
        // Must contain a substantial mix of alphanumeric characters
        let alphanumeric_count = bytes.iter().filter(|b| b.is_ascii_alphanumeric()).count();
        alphanumeric_count >= 40
    }
}

impl<'a> Visit<'a> for FunctionBodyAnalyzer<'a> {
    fn visit_identifier_reference(&mut self, identifier: &oxc_ast::ast::IdentifierReference<'a>) {
        let ref_id = identifier.reference_id.get();
        if identifier.name.as_str() == "atob" && ref_id.map_or(true, |_| true) {
            // atob is a global — it won't have a resolved symbol.
            // Only set if the name matches (unresolved globals have no symbol_id).
            self.has_atob_reference = true;
        }
        self.identifier_references
            .push((identifier.name.to_string(), ref_id));
        walk::walk_identifier_reference(self, identifier);
    }

    fn visit_computed_member_expression(
        &mut self,
        member: &oxc_ast::ast::ComputedMemberExpression<'a>,
    ) {
        if let Expression::Identifier(identifier) = &member.object {
            self.computed_member_object_references
                .push(identifier.reference_id.get());
        }
        walk::walk_computed_member_expression(self, member);
    }

    fn visit_static_member_expression(
        &mut self,
        member: &oxc_ast::ast::StaticMemberExpression<'a>,
    ) {
        if member.property.name.as_str() == "indexOf" {
            self.has_index_of_call = true;
        }
        walk::walk_static_member_expression(self, member);
    }

    fn visit_string_literal(&mut self, literal: &oxc_ast::ast::StringLiteral<'a>) {
        let value = literal.value.as_str();
        if self.custom_base64_alphabet.is_none() && Self::looks_like_base64_alphabet(value) {
            self.custom_base64_alphabet = Some(value.to_string());
        }
        walk::walk_string_literal(self, literal);
    }
}

impl Transformer for StringArrayDecoderTransformer {
    fn name(&self) -> &str {
        "StringArrayDecoderTransformer"
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
        // Phase 1: Find candidate string arrays.
        let mut candidate_arrays: HashMap<SymbolId, Vec<RawArrayElement>> = HashMap::new();

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

                let Some(init) = &declarator.init else {
                    continue;
                };

                if let Some(elements) = Self::extract_array_elements(init) {
                    // Must have at least some string elements to be a string array.
                    let has_strings = elements
                        .iter()
                        .any(|e| matches!(e, RawArrayElement::String(_)));
                    if has_strings {
                        candidate_arrays.insert(symbol_id, elements);
                    }
                }
            }
        }

        if candidate_arrays.is_empty() {
            return false;
        }

        // Phase 2: Find decoder functions that reference these arrays.
        let mut decoders = self.decoders.lock().unwrap();
        let mut found_pairs: Vec<(SymbolId, SymbolId)> = Vec::new(); // (decoder_symbol, array_symbol)

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

            if let Some((array_symbol_id, strategy)) =
                Self::classify_decoder(function, &candidate_arrays, context)
            {
                if let Some(elements) = candidate_arrays.get(&array_symbol_id) {
                    let decoded_values = Self::decode_array(elements, &strategy);
                    decoders.insert(
                        decoder_symbol_id,
                        DecoderInfo { decoded_values },
                    );
                    found_pairs.push((decoder_symbol_id, array_symbol_id));
                }
            }
        }

        if found_pairs.is_empty() {
            return false;
        }

        // Phase 3: Remove array declarations and decoder function declarations.
        let mut changed = false;
        let decoder_symbols: Vec<SymbolId> = found_pairs.iter().map(|(d, _)| *d).collect();
        let array_symbols: Vec<SymbolId> = found_pairs.iter().map(|(_, a)| *a).collect();

        for index in (0..statements.len()).rev() {
            match &statements[index] {
                Statement::FunctionDeclaration(function) => {
                    if let Some(binding) = &function.id {
                        if let Some(symbol_id) = binding.symbol_id.get() {
                            if decoder_symbols.contains(&symbol_id) {
                                operations::remove_statement_at(statements, index, context);
                                changed = true;
                            }
                        }
                    }
                }
                Statement::VariableDeclaration(declaration) => {
                    let all_arrays = declaration.declarations.iter().all(|declarator| {
                        if let oxc_ast::ast::BindingPattern::BindingIdentifier(binding) =
                            &declarator.id
                        {
                            if let Some(symbol_id) = binding.symbol_id.get() {
                                return array_symbols.contains(&symbol_id);
                            }
                        }
                        false
                    });
                    if all_arrays {
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
        let Expression::CallExpression(call) = expression else {
            return false;
        };

        // Check if callee is a reference to a known decoder function.
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

        // Must have exactly 1 or 2 arguments (some decoders take unused second param).
        if call.arguments.is_empty() || call.arguments.len() > 2 {
            return false;
        }

        // First argument must be a numeric literal (the index).
        let Some(first_argument) = call.arguments[0].as_expression() else {
            return false;
        };

        let Some(index) = extract_numeric_index(first_argument) else {
            return false;
        };

        if index >= decoder_info.decoded_values.len() {
            return false;
        }

        // Check if the value at this index came from a non-literal element.
        let value = &decoder_info.decoded_values[index];

        let replacement = Self::build_replacement(value, context);
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

/// Unwrap parenthesized expressions.
fn unwrap_parens<'a, 'b>(expression: &'b Expression<'a>) -> &'b Expression<'a> {
    let mut current = expression;
    while let Expression::ParenthesizedExpression(paren) = current {
        current = &paren.expression;
    }
    current
}
