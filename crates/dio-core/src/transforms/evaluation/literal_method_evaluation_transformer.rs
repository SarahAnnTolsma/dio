//! Evaluates method calls and property accesses on string and array literals
//! with constant arguments.
//!
//! Supported string method calls:
//! - `"hello".charAt(0)` -> `"h"`
//! - `"hello".charCodeAt(0)` -> `104`
//! - `"hello".indexOf("ll")` -> `2`
//! - `"hello".lastIndexOf("l")` -> `3`
//! - `"hello".includes("ell")` -> `true`
//! - `"hello".startsWith("he")` -> `true`
//! - `"hello".endsWith("lo")` -> `true`
//! - `"hello".slice(1, 3)` -> `"el"`
//! - `"hello".substring(1, 3)` -> `"el"`
//! - `"HELLO".toLowerCase()` -> `"hello"`
//! - `"hello".toUpperCase()` -> `"HELLO"`
//! - `"  hello  ".trim()` -> `"hello"`
//! - `"hello".repeat(3)` -> `"hellohellohello"`
//! - `"hello".replace("l", "r")` -> `"herlo"`
//!
//! Supported property accesses:
//! - `"hello".length` -> `5`
//! - `"hello"[0]` -> `"h"`
//! - `[1,2,3].length` -> `3`
//! - `[1,2,3][0]` -> `1`

use oxc_ast::ast::{ArrayExpressionElement, Expression};
use oxc_span::SPAN;
use oxc_syntax::number::NumberBase;
use oxc_traverse::TraverseCtx;

use crate::operations;
use crate::transformer::{AstNodeType, Transformer, TransformerPhase, TransformerPriority};

/// Evaluates method calls and property accesses on string and array literals.
pub struct LiteralMethodEvaluationTransformer;

impl Transformer for LiteralMethodEvaluationTransformer {
    fn name(&self) -> &str {
        "LiteralMethodEvaluationTransformer"
    }

    fn interests(&self) -> &[AstNodeType] {
        &[AstNodeType::CallExpression, AstNodeType::MemberExpression]
    }

    fn priority(&self) -> TransformerPriority {
        TransformerPriority::Default
    }

    fn phase(&self) -> TransformerPhase {
        TransformerPhase::Main
    }

    fn enter_expression<'a>(
        &self,
        expression: &mut Expression<'a>,
        context: &mut TraverseCtx<'a, ()>,
    ) -> bool {
        match expression {
            Expression::CallExpression(_) => {
                try_evaluate_string_method_call(expression, context)
            }
            Expression::StaticMemberExpression(_)
            | Expression::ComputedMemberExpression(_) => {
                try_evaluate_literal_property_access(expression, context)
            }
            _ => false,
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Create a numeric literal expression.
fn make_numeric_literal<'a>(context: &TraverseCtx<'a, ()>, value: f64) -> Expression<'a> {
    let raw = context.ast.atom(&format_number(value));
    context
        .ast
        .expression_numeric_literal(SPAN, value, Some(raw), NumberBase::Decimal)
}

/// Create a string literal expression.
fn make_string_literal<'a>(context: &TraverseCtx<'a, ()>, value: &str) -> Expression<'a> {
    let atom = context.ast.atom(value);
    context.ast.expression_string_literal(SPAN, atom, None)
}

/// Format a number for the raw literal string, omitting `.0` for integers.
fn format_number(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < (i64::MAX as f64) {
        format!("{}", value as i64)
    } else {
        value.to_string()
    }
}

/// Unwrap parenthesized expressions to get the inner expression.
fn unwrap_parens<'a, 'b>(expression: &'b Expression<'a>) -> &'b Expression<'a> {
    let mut current = expression;
    while let Expression::ParenthesizedExpression(paren) = current {
        current = &paren.expression;
    }
    current
}

/// Check whether an array expression element is a literal. Returns `true` for
/// numeric, string, boolean, and null literals.
fn is_literal_element(element: &ArrayExpressionElement<'_>) -> bool {
    match element {
        ArrayExpressionElement::SpreadElement(_) | ArrayExpressionElement::Elision(_) => false,
        _ => {
            let expression = element.to_expression();
            matches!(
                expression,
                Expression::NumericLiteral(_)
                    | Expression::StringLiteral(_)
                    | Expression::BooleanLiteral(_)
                    | Expression::NullLiteral(_)
            )
        }
    }
}

/// Check if all elements of an array expression are literals.
fn all_elements_are_literals(elements: &[ArrayExpressionElement<'_>]) -> bool {
    elements.iter().all(is_literal_element)
}

/// Clone a literal array element into a new expression.
fn clone_literal_element<'a>(
    element: &ArrayExpressionElement<'a>,
    context: &TraverseCtx<'a, ()>,
) -> Option<Expression<'a>> {
    match element {
        ArrayExpressionElement::SpreadElement(_) | ArrayExpressionElement::Elision(_) => {
            return None;
        }
        _ => {}
    }
    let expression = element.to_expression();
    match expression {
        Expression::NumericLiteral(number) => Some(make_numeric_literal(context, number.value)),
        Expression::StringLiteral(string) => {
            Some(make_string_literal(context, string.value.as_str()))
        }
        Expression::BooleanLiteral(boolean) => {
            Some(context.ast.expression_boolean_literal(SPAN, boolean.value))
        }
        Expression::NullLiteral(_) => Some(context.ast.expression_null_literal(SPAN)),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// String method call evaluation (CallExpression handler)
// ---------------------------------------------------------------------------

/// Try to evaluate a string method call like `"hello".charAt(0)`.
fn try_evaluate_string_method_call<'a>(
    expression: &mut Expression<'a>,
    context: &mut TraverseCtx<'a, ()>,
) -> bool {
    let Expression::CallExpression(call) = expression else {
        return false;
    };

    // Callee must be a static member expression on a string literal.
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };

    let Expression::StringLiteral(string_literal) = unwrap_parens(&member.object) else {
        return false;
    };

    let string_value = string_literal.value.as_str().to_owned();
    let method_name = member.property.name.as_str().to_owned();

    match method_name.as_str() {
        "charAt" => try_evaluate_char_at(expression, &string_value, context),
        "charCodeAt" => try_evaluate_char_code_at(expression, &string_value, context),
        "indexOf" => try_evaluate_index_of(expression, &string_value, context),
        "lastIndexOf" => try_evaluate_last_index_of(expression, &string_value, context),
        "includes" => try_evaluate_includes(expression, &string_value, context),
        "startsWith" => try_evaluate_starts_with(expression, &string_value, context),
        "endsWith" => try_evaluate_ends_with(expression, &string_value, context),
        "slice" => try_evaluate_slice(expression, &string_value, context),
        "substring" => try_evaluate_substring(expression, &string_value, context),
        "toLowerCase" => try_evaluate_to_lower_case(expression, &string_value, context),
        "toUpperCase" => try_evaluate_to_upper_case(expression, &string_value, context),
        "trim" => try_evaluate_trim(expression, &string_value, context),
        "repeat" => try_evaluate_repeat(expression, &string_value, context),
        "replace" => try_evaluate_replace(expression, &string_value, context),
        _ => false,
    }
}

/// `"hello".charAt(0)` -> `"h"`
fn try_evaluate_char_at<'a>(
    expression: &mut Expression<'a>,
    string_value: &str,
    context: &mut TraverseCtx<'a, ()>,
) -> bool {
    let Expression::CallExpression(call) = expression else {
        return false;
    };
    if call.arguments.len() != 1 {
        return false;
    }
    let Some(argument) = call.arguments[0].as_expression() else {
        return false;
    };
    let Expression::NumericLiteral(number) = unwrap_parens(argument) else {
        return false;
    };
    let index = number.value as usize;
    let chars: Vec<char> = string_value.chars().collect();
    if index >= chars.len() {
        return false;
    }
    let result = chars[index].to_string();
    let replacement = make_string_literal(context, &result);
    operations::replace_expression(expression, replacement, context);
    true
}

/// `"hello".charCodeAt(0)` -> `104`
fn try_evaluate_char_code_at<'a>(
    expression: &mut Expression<'a>,
    string_value: &str,
    context: &mut TraverseCtx<'a, ()>,
) -> bool {
    let Expression::CallExpression(call) = expression else {
        return false;
    };
    if call.arguments.len() != 1 {
        return false;
    }
    let Some(argument) = call.arguments[0].as_expression() else {
        return false;
    };
    let Expression::NumericLiteral(number) = unwrap_parens(argument) else {
        return false;
    };
    let index = number.value as usize;
    let chars: Vec<char> = string_value.chars().collect();
    if index >= chars.len() {
        return false;
    }
    let code = chars[index] as u32;
    let replacement = make_numeric_literal(context, code as f64);
    operations::replace_expression(expression, replacement, context);
    true
}

/// `"hello".indexOf("ll")` -> `2`
fn try_evaluate_index_of<'a>(
    expression: &mut Expression<'a>,
    string_value: &str,
    context: &mut TraverseCtx<'a, ()>,
) -> bool {
    let Expression::CallExpression(call) = expression else {
        return false;
    };
    if call.arguments.len() != 1 {
        return false;
    }
    let Some(argument) = call.arguments[0].as_expression() else {
        return false;
    };
    let Expression::StringLiteral(search_string) = unwrap_parens(argument) else {
        return false;
    };
    let result = match string_value.find(search_string.value.as_str()) {
        Some(position) => position as f64,
        None => -1.0,
    };
    let replacement = make_numeric_literal(context, result);
    operations::replace_expression(expression, replacement, context);
    true
}

/// `"hello".lastIndexOf("l")` -> `3`
fn try_evaluate_last_index_of<'a>(
    expression: &mut Expression<'a>,
    string_value: &str,
    context: &mut TraverseCtx<'a, ()>,
) -> bool {
    let Expression::CallExpression(call) = expression else {
        return false;
    };
    if call.arguments.len() != 1 {
        return false;
    }
    let Some(argument) = call.arguments[0].as_expression() else {
        return false;
    };
    let Expression::StringLiteral(search_string) = unwrap_parens(argument) else {
        return false;
    };
    let result = match string_value.rfind(search_string.value.as_str()) {
        Some(position) => position as f64,
        None => -1.0,
    };
    let replacement = make_numeric_literal(context, result);
    operations::replace_expression(expression, replacement, context);
    true
}

/// `"hello".includes("ell")` -> `true`
fn try_evaluate_includes<'a>(
    expression: &mut Expression<'a>,
    string_value: &str,
    context: &mut TraverseCtx<'a, ()>,
) -> bool {
    let Expression::CallExpression(call) = expression else {
        return false;
    };
    if call.arguments.len() != 1 {
        return false;
    }
    let Some(argument) = call.arguments[0].as_expression() else {
        return false;
    };
    let Expression::StringLiteral(search_string) = unwrap_parens(argument) else {
        return false;
    };
    let result = string_value.contains(search_string.value.as_str());
    let replacement = context.ast.expression_boolean_literal(SPAN, result);
    operations::replace_expression(expression, replacement, context);
    true
}

/// `"hello".startsWith("he")` -> `true`
fn try_evaluate_starts_with<'a>(
    expression: &mut Expression<'a>,
    string_value: &str,
    context: &mut TraverseCtx<'a, ()>,
) -> bool {
    let Expression::CallExpression(call) = expression else {
        return false;
    };
    if call.arguments.len() != 1 {
        return false;
    }
    let Some(argument) = call.arguments[0].as_expression() else {
        return false;
    };
    let Expression::StringLiteral(search_string) = unwrap_parens(argument) else {
        return false;
    };
    let result = string_value.starts_with(search_string.value.as_str());
    let replacement = context.ast.expression_boolean_literal(SPAN, result);
    operations::replace_expression(expression, replacement, context);
    true
}

/// `"hello".endsWith("lo")` -> `true`
fn try_evaluate_ends_with<'a>(
    expression: &mut Expression<'a>,
    string_value: &str,
    context: &mut TraverseCtx<'a, ()>,
) -> bool {
    let Expression::CallExpression(call) = expression else {
        return false;
    };
    if call.arguments.len() != 1 {
        return false;
    }
    let Some(argument) = call.arguments[0].as_expression() else {
        return false;
    };
    let Expression::StringLiteral(search_string) = unwrap_parens(argument) else {
        return false;
    };
    let result = string_value.ends_with(search_string.value.as_str());
    let replacement = context.ast.expression_boolean_literal(SPAN, result);
    operations::replace_expression(expression, replacement, context);
    true
}

/// `"hello".slice(1, 3)` -> `"el"` (handles negative indices like JavaScript).
fn try_evaluate_slice<'a>(
    expression: &mut Expression<'a>,
    string_value: &str,
    context: &mut TraverseCtx<'a, ()>,
) -> bool {
    let Expression::CallExpression(call) = expression else {
        return false;
    };
    if call.arguments.is_empty() || call.arguments.len() > 2 {
        return false;
    }

    let Some(first_argument) = call.arguments[0].as_expression() else {
        return false;
    };
    let Expression::NumericLiteral(start_number) = unwrap_parens(first_argument) else {
        return false;
    };
    let length = string_value.len() as i64;
    let mut start = start_number.value as i64;

    // Handle negative start index.
    if start < 0 {
        start = (length + start).max(0);
    }
    let start = start.min(length) as usize;

    let end = if call.arguments.len() == 2 {
        let Some(second_argument) = call.arguments[1].as_expression() else {
            return false;
        };
        let Expression::NumericLiteral(end_number) = unwrap_parens(second_argument) else {
            return false;
        };
        let mut end = end_number.value as i64;
        // Handle negative end index.
        if end < 0 {
            end = (length + end).max(0);
        }
        end.min(length) as usize
    } else {
        string_value.len()
    };

    if start > end {
        let replacement = make_string_literal(context, "");
        operations::replace_expression(expression, replacement, context);
        return true;
    }

    // Safety: we operate on byte indices only when the string is ASCII-safe.
    // For full correctness with multi-byte chars, use char-based slicing.
    let chars: Vec<char> = string_value.chars().collect();
    let char_length = chars.len();
    let safe_start = start.min(char_length);
    let safe_end = end.min(char_length);
    let result: String = chars[safe_start..safe_end].iter().collect();

    let replacement = make_string_literal(context, &result);
    operations::replace_expression(expression, replacement, context);
    true
}

/// `"hello".substring(1, 3)` -> `"el"`
fn try_evaluate_substring<'a>(
    expression: &mut Expression<'a>,
    string_value: &str,
    context: &mut TraverseCtx<'a, ()>,
) -> bool {
    let Expression::CallExpression(call) = expression else {
        return false;
    };
    if call.arguments.is_empty() || call.arguments.len() > 2 {
        return false;
    }

    let Some(first_argument) = call.arguments[0].as_expression() else {
        return false;
    };
    let Expression::NumericLiteral(start_number) = unwrap_parens(first_argument) else {
        return false;
    };

    let chars: Vec<char> = string_value.chars().collect();
    let char_length = chars.len();

    // JavaScript substring clamps negatives to 0.
    let mut start = (start_number.value as i64).max(0) as usize;
    start = start.min(char_length);

    let mut end = if call.arguments.len() == 2 {
        let Some(second_argument) = call.arguments[1].as_expression() else {
            return false;
        };
        let Expression::NumericLiteral(end_number) = unwrap_parens(second_argument) else {
            return false;
        };
        let end = (end_number.value as i64).max(0) as usize;
        end.min(char_length)
    } else {
        char_length
    };

    // JavaScript substring swaps start and end if start > end.
    if start > end {
        std::mem::swap(&mut start, &mut end);
    }

    let result: String = chars[start..end].iter().collect();
    let replacement = make_string_literal(context, &result);
    operations::replace_expression(expression, replacement, context);
    true
}

/// `"HELLO".toLowerCase()` -> `"hello"`
fn try_evaluate_to_lower_case<'a>(
    expression: &mut Expression<'a>,
    string_value: &str,
    context: &mut TraverseCtx<'a, ()>,
) -> bool {
    let Expression::CallExpression(call) = expression else {
        return false;
    };
    if !call.arguments.is_empty() {
        return false;
    }
    let result = string_value.to_lowercase();
    let replacement = make_string_literal(context, &result);
    operations::replace_expression(expression, replacement, context);
    true
}

/// `"hello".toUpperCase()` -> `"HELLO"`
fn try_evaluate_to_upper_case<'a>(
    expression: &mut Expression<'a>,
    string_value: &str,
    context: &mut TraverseCtx<'a, ()>,
) -> bool {
    let Expression::CallExpression(call) = expression else {
        return false;
    };
    if !call.arguments.is_empty() {
        return false;
    }
    let result = string_value.to_uppercase();
    let replacement = make_string_literal(context, &result);
    operations::replace_expression(expression, replacement, context);
    true
}

/// `"  hello  ".trim()` -> `"hello"`
fn try_evaluate_trim<'a>(
    expression: &mut Expression<'a>,
    string_value: &str,
    context: &mut TraverseCtx<'a, ()>,
) -> bool {
    let Expression::CallExpression(call) = expression else {
        return false;
    };
    if !call.arguments.is_empty() {
        return false;
    }
    let result = string_value.trim();
    let replacement = make_string_literal(context, result);
    operations::replace_expression(expression, replacement, context);
    true
}

/// `"hello".repeat(3)` -> `"hellohellohello"`
fn try_evaluate_repeat<'a>(
    expression: &mut Expression<'a>,
    string_value: &str,
    context: &mut TraverseCtx<'a, ()>,
) -> bool {
    let Expression::CallExpression(call) = expression else {
        return false;
    };
    if call.arguments.len() != 1 {
        return false;
    }
    let Some(argument) = call.arguments[0].as_expression() else {
        return false;
    };
    let Expression::NumericLiteral(count_number) = unwrap_parens(argument) else {
        return false;
    };
    let count = count_number.value as usize;
    // Reject unreasonably large repeat counts.
    if count > 1000 {
        return false;
    }
    let result = string_value.repeat(count);
    let replacement = make_string_literal(context, &result);
    operations::replace_expression(expression, replacement, context);
    true
}

/// `"hello".replace("l", "r")` -> `"herlo"` (first occurrence only).
fn try_evaluate_replace<'a>(
    expression: &mut Expression<'a>,
    string_value: &str,
    context: &mut TraverseCtx<'a, ()>,
) -> bool {
    let Expression::CallExpression(call) = expression else {
        return false;
    };
    if call.arguments.len() != 2 {
        return false;
    }
    let Some(search_argument) = call.arguments[0].as_expression() else {
        return false;
    };
    let Expression::StringLiteral(search_string) = unwrap_parens(search_argument) else {
        return false;
    };
    let Some(replace_argument) = call.arguments[1].as_expression() else {
        return false;
    };
    let Expression::StringLiteral(replace_string) = unwrap_parens(replace_argument) else {
        return false;
    };

    let search = search_string.value.as_str();
    let replace = replace_string.value.as_str();

    // Replace first occurrence only, matching JavaScript's String.prototype.replace
    // with a string pattern.
    let result = if let Some(position) = string_value.find(search) {
        let mut result = String::with_capacity(string_value.len() - search.len() + replace.len());
        result.push_str(&string_value[..position]);
        result.push_str(replace);
        result.push_str(&string_value[position + search.len()..]);
        result
    } else {
        string_value.to_owned()
    };

    let replacement = make_string_literal(context, &result);
    operations::replace_expression(expression, replacement, context);
    true
}

// ---------------------------------------------------------------------------
// Literal property access evaluation (MemberExpression handler)
// ---------------------------------------------------------------------------

/// Try to evaluate property access on a literal (string or array).
fn try_evaluate_literal_property_access<'a>(
    expression: &mut Expression<'a>,
    context: &mut TraverseCtx<'a, ()>,
) -> bool {
    match expression {
        Expression::StaticMemberExpression(_) => {
            try_evaluate_static_member_access(expression, context)
        }
        Expression::ComputedMemberExpression(_) => {
            try_evaluate_computed_member_access(expression, context)
        }
        _ => false,
    }
}

/// Evaluate static property access like `"hello".length` or `[1,2,3].length`.
fn try_evaluate_static_member_access<'a>(
    expression: &mut Expression<'a>,
    context: &mut TraverseCtx<'a, ()>,
) -> bool {
    let Expression::StaticMemberExpression(member) = expression else {
        return false;
    };

    let property_name = member.property.name.as_str();
    if property_name != "length" {
        return false;
    }

    let object = unwrap_parens(&member.object);

    // `"hello".length` -> `5`
    if let Expression::StringLiteral(string_literal) = object {
        let length = string_literal.value.as_str().len() as f64;
        let replacement = make_numeric_literal(context, length);
        operations::replace_expression(expression, replacement, context);
        return true;
    }

    // `[1,2,3].length` -> `3` (only when all elements are literals).
    if let Expression::ArrayExpression(array) = object {
        if all_elements_are_literals(&array.elements) {
            let length = array.elements.len() as f64;
            let replacement = make_numeric_literal(context, length);
            operations::replace_expression(expression, replacement, context);
            return true;
        }
    }

    false
}

/// Evaluate computed property access like `"hello"[0]` or `[1,2,3][0]`.
fn try_evaluate_computed_member_access<'a>(
    expression: &mut Expression<'a>,
    context: &mut TraverseCtx<'a, ()>,
) -> bool {
    let Expression::ComputedMemberExpression(member) = expression else {
        return false;
    };

    let index_expression = unwrap_parens(&member.expression);
    let Expression::NumericLiteral(index_number) = index_expression else {
        return false;
    };
    let index_value = index_number.value;

    // Only handle non-negative integer indices.
    if index_value < 0.0 || index_value.fract() != 0.0 {
        return false;
    }
    let index = index_value as usize;

    let object = unwrap_parens(&member.object);

    // `"hello"[0]` -> `"h"`
    if let Expression::StringLiteral(string_literal) = object {
        let chars: Vec<char> = string_literal.value.as_str().chars().collect();
        if index >= chars.len() {
            return false;
        }
        let result = chars[index].to_string();
        let replacement = make_string_literal(context, &result);
        operations::replace_expression(expression, replacement, context);
        return true;
    }

    // `[1,2,3][0]` -> `1` (only when all elements are literals).
    if let Expression::ArrayExpression(array) = object {
        if !all_elements_are_literals(&array.elements) {
            return false;
        }
        if index >= array.elements.len() {
            return false;
        }
        let Some(replacement) = clone_literal_element(&array.elements[index], context) else {
            return false;
        };
        operations::replace_expression(expression, replacement, context);
        return true;
    }

    false
}
