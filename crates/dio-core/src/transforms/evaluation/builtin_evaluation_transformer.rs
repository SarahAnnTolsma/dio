//! Evaluates calls to known pure built-in functions with constant arguments.
//!
//! Supported functions:
//! - `String.fromCharCode(72, 101, 108)` -> `"Hel"`
//! - `parseInt("1a", 16)` / `Number.parseInt("1a", 16)` -> `26`
//! - `parseFloat("3.14")` / `Number.parseFloat("3.14")` -> `3.14`
//! - `Number("42")` / `Number(true)` -> `42` / `1`
//! - `Boolean(1)` / `Boolean("")` -> `true` / `false`
//! - `atob("aGVsbG8=")` -> `"hello"`
//! - `btoa("hello")` -> `"aGVsbG8="`
//! - `Math.ceil(1.5)` -> `2`, `Math.floor(1.9)` -> `1`, `Math.round(1.5)` -> `2`
//! - `Math.abs(-5)` -> `5`, `Math.trunc(1.9)` -> `1`
//! - `Math.min(1, 2)` -> `1`, `Math.max(1, 2)` -> `2`
//! - `Math.sign(-5)` -> `-1`, `Math.sqrt(9)` -> `3`

use oxc_ast::ast::Expression;
use oxc_span::SPAN;
use oxc_syntax::number::NumberBase;
use oxc_traverse::TraverseCtx;

use crate::operations;
use crate::transformer::{AstNodeType, Transformer, TransformerPhase, TransformerPriority};

/// Evaluates known built-in function calls with constant arguments.
pub struct BuiltinEvaluationTransformer;

impl Transformer for BuiltinEvaluationTransformer {
    fn name(&self) -> &str {
        "BuiltinEvaluationTransformer"
    }

    fn interests(&self) -> &[AstNodeType] {
        &[AstNodeType::CallExpression]
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
        let Expression::CallExpression(call) = expression else {
            return false;
        };

        // Check for static member calls: String.fromCharCode, Number.parseInt, Number.parseFloat.
        if is_static_member_call(&call.callee, "String", "fromCharCode") {
            return try_evaluate_from_char_code(expression, context);
        }
        if is_static_member_call(&call.callee, "Number", "parseInt") {
            return try_evaluate_parse_int(expression, context);
        }
        if is_static_member_call(&call.callee, "Number", "parseFloat") {
            return try_evaluate_parse_float(expression, context);
        }

        // Check for Math method calls. Copy the method name to avoid borrow conflict.
        if let Some(method_name) = get_static_member_method(&call.callee, "Math") {
            let method_name = method_name.to_string();
            return try_evaluate_math_method(expression, &method_name, context);
        }

        // Check for global function calls.
        if let Expression::Identifier(identifier) = &call.callee {
            match identifier.name.as_str() {
                "parseInt" => return try_evaluate_parse_int(expression, context),
                "parseFloat" => return try_evaluate_parse_float(expression, context),
                "Number" => return try_evaluate_number(expression, context),
                "Boolean" => return try_evaluate_boolean(expression, context),
                "atob" => return try_evaluate_atob(expression, context),
                "btoa" => return try_evaluate_btoa(expression, context),
                _ => {}
            }
        }

        false
    }
}

/// Check if the callee is `ObjectName.methodName`.
fn is_static_member_call(callee: &Expression<'_>, object_name: &str, method_name: &str) -> bool {
    if let Expression::StaticMemberExpression(member) = callee
        && member.property.name.as_str() == method_name
            && let Expression::Identifier(identifier) = &member.object {
                return identifier.name.as_str() == object_name;
            }
    false
}

/// If the callee is `ObjectName.methodName`, return the method name.
fn get_static_member_method<'a>(
    callee: &'a Expression<'_>,
    object_name: &str,
) -> Option<&'a str> {
    if let Expression::StaticMemberExpression(member) = callee
        && let Expression::Identifier(identifier) = &member.object
            && identifier.name.as_str() == object_name {
                return Some(member.property.name.as_str());
            }
    None
}

/// `String.fromCharCode(72, 101, 108)` -> `"Hel"`
fn try_evaluate_from_char_code<'a>(
    expression: &mut Expression<'a>,
    context: &mut TraverseCtx<'a, ()>,
) -> bool {
    let Expression::CallExpression(call) = expression else {
        return false;
    };

    let mut chars = Vec::new();
    for argument in &call.arguments {
        let Some(argument_expression) = argument.as_expression() else {
            return false;
        };
        let Expression::NumericLiteral(number) = argument_expression else {
            return false;
        };
        let Some(character) = char::from_u32(number.value as u32) else {
            return false;
        };
        chars.push(character);
    }

    let result: String = chars.into_iter().collect();
    let value = context.ast.atom(&result);
    let replacement = context.ast.expression_string_literal(SPAN, value, None);
    operations::replace_expression(expression, replacement, context);
    true
}

/// `parseInt("1a", 16)` -> `26`
fn try_evaluate_parse_int<'a>(
    expression: &mut Expression<'a>,
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

    // parseInt with a numeric literal: truncate to integer.
    // JS converts to string first, so parseInt(123.7) → 123.
    if let Expression::NumericLiteral(number) = first_argument {
        if call.arguments.len() > 1 {
            // parseInt(numericLiteral, radix) is unusual — skip to be safe.
            return false;
        }
        let value = number.value;
        if !value.is_finite() {
            return false;
        }
        let result = value.trunc() as i64;
        let result_f64 = result as f64;
        let raw = context.ast.atom(&result.to_string());
        let replacement =
            context
                .ast
                .expression_numeric_literal(SPAN, result_f64, Some(raw), NumberBase::Decimal);
        operations::replace_expression(expression, replacement, context);
        return true;
    }

    let Expression::StringLiteral(string_value) = first_argument else {
        return false;
    };
    let input = string_value.value.as_str().trim();

    let radix: u32 = if call.arguments.len() == 2 {
        let Some(second_argument) = call.arguments[1].as_expression() else {
            return false;
        };
        let Expression::NumericLiteral(radix_number) = second_argument else {
            return false;
        };
        let radix = radix_number.value as u32;
        if !(2..=36).contains(&radix) {
            return false;
        }
        radix
    } else {
        10
    };

    let Some(result) = parse_int_with_radix(input, radix) else {
        return false;
    };

    let result_f64 = result as f64;
    let raw = context.ast.atom(&result.to_string());
    let replacement =
        context
            .ast
            .expression_numeric_literal(SPAN, result_f64, Some(raw), NumberBase::Decimal);
    operations::replace_expression(expression, replacement, context);
    true
}

/// Parse an integer string with the given radix, matching JavaScript's parseInt behavior.
fn parse_int_with_radix(input: &str, radix: u32) -> Option<i64> {
    let input = input.trim();
    if input.is_empty() {
        return None;
    }

    let (is_negative, digits) = if let Some(rest) = input.strip_prefix('-') {
        (true, rest)
    } else if let Some(rest) = input.strip_prefix('+') {
        (false, rest)
    } else {
        (false, input)
    };

    let valid_prefix: String = digits
        .chars()
        .take_while(|character| character.is_digit(radix))
        .collect();

    if valid_prefix.is_empty() {
        return None;
    }

    let value = i64::from_str_radix(&valid_prefix, radix).ok()?;
    Some(if is_negative { -value } else { value })
}

/// `parseFloat("3.14")` -> `3.14`
fn try_evaluate_parse_float<'a>(
    expression: &mut Expression<'a>,
    context: &mut TraverseCtx<'a, ()>,
) -> bool {
    let Expression::CallExpression(call) = expression else {
        return false;
    };

    if call.arguments.len() != 1 {
        return false;
    }

    let Some(first_argument) = call.arguments[0].as_expression() else {
        return false;
    };

    // parseFloat with a numeric literal is an identity operation.
    if let Expression::NumericLiteral(number) = first_argument {
        let value = number.value;
        if !value.is_finite() {
            return false;
        }
        let raw = context.ast.atom(&format_number(value));
        let replacement =
            context
                .ast
                .expression_numeric_literal(SPAN, value, Some(raw), NumberBase::Decimal);
        operations::replace_expression(expression, replacement, context);
        return true;
    }

    let Expression::StringLiteral(string_value) = first_argument else {
        return false;
    };

    let Ok(result) = string_value.value.as_str().trim().parse::<f64>() else {
        return false;
    };

    if !result.is_finite() {
        return false;
    }

    let raw = context.ast.atom(&result.to_string());
    let replacement =
        context
            .ast
            .expression_numeric_literal(SPAN, result, Some(raw), NumberBase::Decimal);
    operations::replace_expression(expression, replacement, context);
    true
}

/// `atob("aGVsbG8=")` -> `"hello"`
fn try_evaluate_atob<'a>(
    expression: &mut Expression<'a>,
    context: &mut TraverseCtx<'a, ()>,
) -> bool {
    let Expression::CallExpression(call) = expression else {
        return false;
    };

    if call.arguments.len() != 1 {
        return false;
    }

    let Some(first_argument) = call.arguments[0].as_expression() else {
        return false;
    };
    let Expression::StringLiteral(string_value) = first_argument else {
        return false;
    };

    let Some(decoded_bytes) = base64_decode(string_value.value.as_str()) else {
        return false;
    };

    let result: String = decoded_bytes.iter().map(|&byte| byte as char).collect();
    let value = context.ast.atom(&result);
    let replacement = context.ast.expression_string_literal(SPAN, value, None);
    operations::replace_expression(expression, replacement, context);
    true
}

/// `btoa("hello")` -> `"aGVsbG8="`
fn try_evaluate_btoa<'a>(
    expression: &mut Expression<'a>,
    context: &mut TraverseCtx<'a, ()>,
) -> bool {
    let Expression::CallExpression(call) = expression else {
        return false;
    };

    if call.arguments.len() != 1 {
        return false;
    }

    let Some(first_argument) = call.arguments[0].as_expression() else {
        return false;
    };
    let Expression::StringLiteral(string_value) = first_argument else {
        return false;
    };

    let input = string_value.value.as_str();
    if input.chars().any(|character| character as u32 > 0xFF) {
        return false;
    }

    let bytes: Vec<u8> = input.bytes().collect();
    let encoded = base64_encode(&bytes);
    let value = context.ast.atom(&encoded);
    let replacement = context.ast.expression_string_literal(SPAN, value, None);
    operations::replace_expression(expression, replacement, context);
    true
}

/// `Number("42")` -> `42`, `Number(true)` -> `1`, `Number(false)` -> `0`, `Number(null)` -> `0`
fn try_evaluate_number<'a>(
    expression: &mut Expression<'a>,
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

    let result = match argument {
        Expression::StringLiteral(string) => {
            let trimmed = string.value.as_str().trim();
            if trimmed.is_empty() {
                0.0
            } else {
                let Ok(value) = trimmed.parse::<f64>() else {
                    return false;
                };
                if !value.is_finite() {
                    return false;
                }
                value
            }
        }
        Expression::NumericLiteral(number) => number.value,
        Expression::BooleanLiteral(boolean) => {
            if boolean.value {
                1.0
            } else {
                0.0
            }
        }
        Expression::NullLiteral(_) => 0.0,
        _ => return false,
    };

    let raw = context.ast.atom(&format_number(result));
    let replacement =
        context
            .ast
            .expression_numeric_literal(SPAN, result, Some(raw), NumberBase::Decimal);
    operations::replace_expression(expression, replacement, context);
    true
}

/// `Boolean(1)` -> `true`, `Boolean(0)` -> `false`, `Boolean("")` -> `false`, etc.
fn try_evaluate_boolean<'a>(
    expression: &mut Expression<'a>,
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

    let result = match argument {
        Expression::NumericLiteral(number) => number.value != 0.0 && !number.value.is_nan(),
        Expression::StringLiteral(string) => !string.value.is_empty(),
        Expression::BooleanLiteral(boolean) => boolean.value,
        Expression::NullLiteral(_) => false,
        _ => return false,
    };

    let replacement = context.ast.expression_boolean_literal(SPAN, result);
    operations::replace_expression(expression, replacement, context);
    true
}

/// Evaluates `Math.*` method calls with numeric literal arguments.
fn try_evaluate_math_method<'a>(
    expression: &mut Expression<'a>,
    method_name: &str,
    context: &mut TraverseCtx<'a, ()>,
) -> bool {
    let Expression::CallExpression(call) = expression else {
        return false;
    };

    if call.arguments.is_empty() {
        return false;
    }

    // Extract all numeric arguments.
    let mut numeric_arguments = Vec::new();
    for argument in &call.arguments {
        let Some(argument_expression) = argument.as_expression() else {
            return false;
        };
        let Expression::NumericLiteral(number) = argument_expression else {
            return false;
        };
        numeric_arguments.push(number.value);
    }

    let result = match method_name {
        // Single-argument methods.
        "ceil" if numeric_arguments.len() == 1 => numeric_arguments[0].ceil(),
        "floor" if numeric_arguments.len() == 1 => numeric_arguments[0].floor(),
        "round" if numeric_arguments.len() == 1 => numeric_arguments[0].round(),
        "abs" if numeric_arguments.len() == 1 => numeric_arguments[0].abs(),
        "trunc" if numeric_arguments.len() == 1 => numeric_arguments[0].trunc(),
        "sign" if numeric_arguments.len() == 1 => {
            let value = numeric_arguments[0];
            if value > 0.0 {
                1.0
            } else if value < 0.0 {
                -1.0
            } else {
                value // preserves +0/-0/NaN
            }
        }
        "sqrt" if numeric_arguments.len() == 1 => {
            let result = numeric_arguments[0].sqrt();
            if !result.is_finite() {
                return false;
            }
            result
        }
        "log" if numeric_arguments.len() == 1 => {
            let result = numeric_arguments[0].ln();
            if !result.is_finite() {
                return false;
            }
            result
        }
        "log2" if numeric_arguments.len() == 1 => {
            let result = numeric_arguments[0].log2();
            if !result.is_finite() {
                return false;
            }
            result
        }
        "log10" if numeric_arguments.len() == 1 => {
            let result = numeric_arguments[0].log10();
            if !result.is_finite() {
                return false;
            }
            result
        }
        // Multi-argument methods.
        "min" if !numeric_arguments.is_empty() => {
            numeric_arguments.iter().copied().fold(f64::INFINITY, f64::min)
        }
        "max" if !numeric_arguments.is_empty() => {
            numeric_arguments
                .iter()
                .copied()
                .fold(f64::NEG_INFINITY, f64::max)
        }
        "pow" if numeric_arguments.len() == 2 => {
            let result = numeric_arguments[0].powf(numeric_arguments[1]);
            if !result.is_finite() {
                return false;
            }
            result
        }
        _ => return false,
    };

    if !result.is_finite() {
        return false;
    }

    let raw = context.ast.atom(&format_number(result));
    let replacement =
        context
            .ast
            .expression_numeric_literal(SPAN, result, Some(raw), NumberBase::Decimal);
    operations::replace_expression(expression, replacement, context);
    true
}

/// Format a number for the raw literal string, omitting `.0` for integers.
fn format_number(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < (i64::MAX as f64) {
        format!("{}", value as i64)
    } else {
        value.to_string()
    }
}

// -- Minimal base64 implementation to avoid adding a dependency --

const BASE64_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64_decode(input: &str) -> Option<Vec<u8>> {
    let input = input.trim_end_matches('=');
    let mut output = Vec::new();
    let mut buffer: u32 = 0;
    let mut bits_collected: u32 = 0;

    for byte in input.bytes() {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b' ' | b'\n' | b'\r' | b'\t' => continue,
            _ => return None,
        };

        buffer = (buffer << 6) | u32::from(value);
        bits_collected += 6;

        if bits_collected >= 8 {
            bits_collected -= 8;
            output.push((buffer >> bits_collected) as u8);
            buffer &= (1 << bits_collected) - 1;
        }
    }

    Some(output)
}

fn base64_encode(input: &[u8]) -> String {
    let mut output = String::new();

    for chunk in input.chunks(3) {
        let b0 = u32::from(chunk[0]);
        let b1 = if chunk.len() > 1 {
            u32::from(chunk[1])
        } else {
            0
        };
        let b2 = if chunk.len() > 2 {
            u32::from(chunk[2])
        } else {
            0
        };

        let triple = (b0 << 16) | (b1 << 8) | b2;

        output.push(BASE64_ALPHABET[((triple >> 18) & 0x3F) as usize] as char);
        output.push(BASE64_ALPHABET[((triple >> 12) & 0x3F) as usize] as char);

        if chunk.len() > 1 {
            output.push(BASE64_ALPHABET[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            output.push('=');
        }

        if chunk.len() > 2 {
            output.push(BASE64_ALPHABET[(triple & 0x3F) as usize] as char);
        } else {
            output.push('=');
        }
    }

    output
}
