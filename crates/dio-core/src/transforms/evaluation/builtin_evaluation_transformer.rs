//! Evaluates calls to known pure built-in functions with constant arguments.
//!
//! Supported functions:
//! - `String.fromCharCode(72, 101, 108)` -> `"Hel"`
//! - `parseInt("1a", 16)` -> `26`
//! - `parseFloat("3.14")` -> `3.14`
//! - `atob("aGVsbG8=")` -> `"hello"`
//! - `btoa("hello")` -> `"aGVsbG8="`

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

        // Check for String.fromCharCode(...numbers).
        if is_static_member_call(&call.callee, "String", "fromCharCode") {
            return try_evaluate_from_char_code(expression, context);
        }

        // Check for global function calls.
        if let Expression::Identifier(identifier) = &call.callee {
            match identifier.name.as_str() {
                "parseInt" => return try_evaluate_parse_int(expression, context),
                "parseFloat" => return try_evaluate_parse_float(expression, context),
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
    if let Expression::StaticMemberExpression(member) = callee {
        if member.property.name.as_str() == method_name {
            if let Expression::Identifier(identifier) = &member.object {
                return identifier.name.as_str() == object_name;
            }
        }
    }
    false
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
