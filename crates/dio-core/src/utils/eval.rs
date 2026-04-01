//! Pure expression evaluation — computes constant values from AST expressions
//! without mutating the tree.
//!
//! Used by transformers that need to evaluate expressions to decide whether
//! to fold, inline, or simulate (e.g., constant folding, checksum computation).

use oxc_ast::ast::Expression;
use oxc_syntax::operator::{BinaryOperator, LogicalOperator, UnaryOperator};

use super::{base64_decode, base64_encode};

/// A JavaScript value produced by constant evaluation.
#[derive(Debug, Clone, PartialEq)]
pub enum JsValue {
    Number(f64),
    String(String),
    Boolean(bool),
    Null,
    Undefined,
}

impl JsValue {
    /// JavaScript truthiness: determines if a value is truthy or falsy.
    pub fn is_truthy(&self) -> bool {
        match self {
            JsValue::Number(n) => *n != 0.0 && !n.is_nan(),
            JsValue::String(s) => !s.is_empty(),
            JsValue::Boolean(b) => *b,
            JsValue::Null | JsValue::Undefined => false,
        }
    }

    /// Coerce to f64, matching JavaScript's `Number(value)` semantics.
    pub fn to_number(&self) -> Option<f64> {
        match self {
            JsValue::Number(n) => Some(*n),
            JsValue::Boolean(true) => Some(1.0),
            JsValue::Boolean(false) => Some(0.0),
            JsValue::Null => Some(0.0),
            JsValue::Undefined => Some(f64::NAN),
            JsValue::String(s) => {
                let trimmed = s.trim();
                if trimmed.is_empty() {
                    Some(0.0)
                } else {
                    trimmed.parse::<f64>().ok()
                }
            }
        }
    }

    /// Extract as f64 if this is a Number.
    pub fn as_number(&self) -> Option<f64> {
        if let JsValue::Number(n) = self {
            Some(*n)
        } else {
            None
        }
    }

    /// Extract as &str if this is a String.
    pub fn as_str(&self) -> Option<&str> {
        if let JsValue::String(s) = self {
            Some(s)
        } else {
            None
        }
    }
}

/// Try to evaluate an expression to a constant value.
///
/// Returns `None` if the expression cannot be statically evaluated
/// (e.g., it contains identifiers, function calls to unknown functions, etc.).
pub fn try_eval(expression: &Expression<'_>) -> Option<JsValue> {
    let expression = unwrap_parens(expression);

    match expression {
        // Literals
        Expression::NumericLiteral(literal) => Some(JsValue::Number(literal.value)),
        Expression::StringLiteral(literal) => Some(JsValue::String(literal.value.to_string())),
        Expression::BooleanLiteral(literal) => Some(JsValue::Boolean(literal.value)),
        Expression::NullLiteral(_) => Some(JsValue::Null),

        // Unary expressions
        Expression::UnaryExpression(unary) => eval_unary(unary.operator, &unary.argument),

        // Binary expressions
        Expression::BinaryExpression(binary) => {
            eval_binary(binary.operator, &binary.left, &binary.right)
        }

        // Logical expressions
        Expression::LogicalExpression(logical) => {
            eval_logical(logical.operator, &logical.left, &logical.right)
        }

        // Known pure function calls
        Expression::CallExpression(call) => eval_call(call),

        // Array/object literals have known truthiness but not a simple scalar value
        Expression::ArrayExpression(_) | Expression::ObjectExpression(_) => None,

        _ => None,
    }
}

/// Evaluate the static truthiness of an expression.
///
/// Returns `Some(true)` if definitely truthy, `Some(false)` if definitely falsy,
/// `None` if unknown. Unlike `try_eval`, this also handles arrays and objects
/// (which are always truthy but don't have a simple scalar value).
pub fn static_truthiness(expression: &Expression<'_>) -> Option<bool> {
    let expression = unwrap_parens(expression);

    // Arrays and objects are always truthy.
    if matches!(
        expression,
        Expression::ArrayExpression(_) | Expression::ObjectExpression(_)
    ) {
        return Some(true);
    }

    try_eval(expression).map(|v| v.is_truthy())
}

/// Simulate JavaScript's `parseInt(string, radix?)`.
///
/// Extracts leading digits from a string, matching JS behavior where
/// `parseInt("123abc")` returns `123`.
pub fn js_parse_int(string: &str, radix: Option<u32>) -> Option<f64> {
    let radix = radix.unwrap_or(10);
    if !(2..=36).contains(&radix) {
        return None;
    }

    let trimmed = string.trim();
    let mut chars = trimmed.chars().peekable();

    // Optional sign.
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

    // Handle 0x prefix for radix 16.
    if radix == 16
        && let Some(&'0') = chars.peek()
    {
        let mut lookahead = chars.clone();
        lookahead.next();
        if matches!(lookahead.peek(), Some('x') | Some('X')) {
            chars.next();
            chars.next();
        }
    }

    // Collect valid digits for the given radix.
    let mut value: i64 = 0;
    let mut found_digit = false;

    for c in chars {
        let digit = c.to_digit(radix);
        match digit {
            Some(d) => {
                value = value * (radix as i64) + (d as i64);
                found_digit = true;
            }
            None => break,
        }
    }

    if !found_digit {
        return None;
    }

    Some(if negative {
        -(value as f64)
    } else {
        value as f64
    })
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn eval_unary(operator: UnaryOperator, argument: &Expression<'_>) -> Option<JsValue> {
    match operator {
        UnaryOperator::LogicalNot => {
            let truthiness = static_truthiness(argument)?;
            Some(JsValue::Boolean(!truthiness))
        }

        UnaryOperator::UnaryNegation => {
            let value = try_eval(argument)?;
            let number = value.to_number()?;
            Some(JsValue::Number(-number))
        }

        UnaryOperator::UnaryPlus => {
            let value = try_eval(argument)?;
            // Special case: +[] = 0
            if matches!(unwrap_parens(argument), Expression::ArrayExpression(a) if a.elements.is_empty())
            {
                return Some(JsValue::Number(0.0));
            }
            let number = value.to_number()?;
            Some(JsValue::Number(number))
        }

        UnaryOperator::Typeof => {
            let expression = unwrap_parens(argument);
            let type_name = match expression {
                Expression::NumericLiteral(_) => "number",
                Expression::StringLiteral(_) => "string",
                Expression::BooleanLiteral(_) => "boolean",
                Expression::NullLiteral(_) => "object",
                Expression::FunctionExpression(_) | Expression::ArrowFunctionExpression(_) => {
                    "function"
                }
                _ => return None,
            };
            Some(JsValue::String(type_name.to_string()))
        }

        UnaryOperator::Void => {
            // void <expr> → undefined (if expr is side-effect-free)
            if is_side_effect_free(argument) {
                Some(JsValue::Undefined)
            } else {
                None
            }
        }

        UnaryOperator::BitwiseNot => {
            let value = try_eval(argument)?;
            let number = value.as_number()?;
            Some(JsValue::Number(f64::from(!(number as i32))))
        }

        _ => None,
    }
}

fn eval_binary(
    operator: BinaryOperator,
    left: &Expression<'_>,
    right: &Expression<'_>,
) -> Option<JsValue> {
    let left_value = try_eval(left)?;
    let right_value = try_eval(right)?;

    // String concatenation.
    if operator == BinaryOperator::Addition
        && let (JsValue::String(l), JsValue::String(r)) = (&left_value, &right_value)
    {
        return Some(JsValue::String(format!("{l}{r}")));
    }

    // Numeric operations.
    let left_number = left_value.as_number()?;
    let right_number = right_value.as_number()?;

    match operator {
        BinaryOperator::Addition => Some(JsValue::Number(left_number + right_number)),
        BinaryOperator::Subtraction => Some(JsValue::Number(left_number - right_number)),
        BinaryOperator::Multiplication => Some(JsValue::Number(left_number * right_number)),
        BinaryOperator::Division => {
            if right_number == 0.0 {
                None
            } else {
                Some(JsValue::Number(left_number / right_number))
            }
        }
        BinaryOperator::Remainder => {
            if right_number == 0.0 {
                None
            } else {
                Some(JsValue::Number(left_number % right_number))
            }
        }
        BinaryOperator::Exponential => Some(JsValue::Number(left_number.powf(right_number))),

        // Comparisons
        BinaryOperator::LessThan => Some(JsValue::Boolean(left_number < right_number)),
        BinaryOperator::LessEqualThan => Some(JsValue::Boolean(left_number <= right_number)),
        BinaryOperator::GreaterThan => Some(JsValue::Boolean(left_number > right_number)),
        BinaryOperator::GreaterEqualThan => Some(JsValue::Boolean(left_number >= right_number)),
        BinaryOperator::StrictEquality => Some(JsValue::Boolean(left_number == right_number)),
        BinaryOperator::StrictInequality => Some(JsValue::Boolean(left_number != right_number)),
        BinaryOperator::Equality => Some(JsValue::Boolean(left_number == right_number)),
        BinaryOperator::Inequality => Some(JsValue::Boolean(left_number != right_number)),

        // Bitwise (JavaScript uses i32 semantics)
        BinaryOperator::BitwiseOR => Some(JsValue::Number(f64::from(
            (left_number as i32) | (right_number as i32),
        ))),
        BinaryOperator::BitwiseAnd => Some(JsValue::Number(f64::from(
            (left_number as i32) & (right_number as i32),
        ))),
        BinaryOperator::BitwiseXOR => Some(JsValue::Number(f64::from(
            (left_number as i32) ^ (right_number as i32),
        ))),
        BinaryOperator::ShiftLeft => Some(JsValue::Number(f64::from(
            (left_number as i32) << ((right_number as u32) & 0x1F),
        ))),
        BinaryOperator::ShiftRight => Some(JsValue::Number(f64::from(
            (left_number as i32) >> ((right_number as u32) & 0x1F),
        ))),
        BinaryOperator::ShiftRightZeroFill => Some(JsValue::Number(f64::from(
            ((left_number as u32) >> ((right_number as u32) & 0x1F)) as i32,
        ))),

        _ => None,
    }
}

fn eval_logical(
    operator: LogicalOperator,
    left: &Expression<'_>,
    right: &Expression<'_>,
) -> Option<JsValue> {
    match operator {
        LogicalOperator::And => {
            let left_value = try_eval(left)?;
            if !left_value.is_truthy() {
                Some(left_value)
            } else {
                try_eval(right)
            }
        }
        LogicalOperator::Or => {
            let left_value = try_eval(left)?;
            if left_value.is_truthy() {
                Some(left_value)
            } else {
                try_eval(right)
            }
        }
        LogicalOperator::Coalesce => None,
    }
}

fn eval_call(call: &oxc_ast::ast::CallExpression<'_>) -> Option<JsValue> {
    let callee = unwrap_parens(&call.callee);

    match callee {
        // parseInt(string) or parseInt(string, radix)
        Expression::Identifier(id) if id.name.as_str() == "parseInt" => {
            if call.arguments.is_empty() || call.arguments.len() > 2 {
                return None;
            }
            let arg = try_eval(call.arguments[0].as_expression()?)?;
            let string = match &arg {
                JsValue::String(s) => s.clone(),
                JsValue::Number(n) => n.to_string(),
                _ => return None,
            };
            let radix = if call.arguments.len() == 2 {
                let r = try_eval(call.arguments[1].as_expression()?)?.as_number()?;
                Some(r as u32)
            } else {
                None
            };
            js_parse_int(&string, radix).map(JsValue::Number)
        }

        // parseFloat(string)
        Expression::Identifier(id) if id.name.as_str() == "parseFloat" => {
            if call.arguments.len() != 1 {
                return None;
            }
            let arg = try_eval(call.arguments[0].as_expression()?)?;
            let string = arg.as_str()?;
            let result: f64 = string.trim().parse().ok()?;
            if result.is_finite() {
                Some(JsValue::Number(result))
            } else {
                None
            }
        }

        // Number(value)
        Expression::Identifier(id) if id.name.as_str() == "Number" => {
            if call.arguments.len() != 1 {
                return None;
            }
            let arg = try_eval(call.arguments[0].as_expression()?)?;
            arg.to_number().map(JsValue::Number)
        }

        // Boolean(value)
        Expression::Identifier(id) if id.name.as_str() == "Boolean" => {
            if call.arguments.len() != 1 {
                return None;
            }
            let arg = try_eval(call.arguments[0].as_expression()?)?;
            Some(JsValue::Boolean(arg.is_truthy()))
        }

        // atob(string)
        Expression::Identifier(id) if id.name.as_str() == "atob" => {
            if call.arguments.len() != 1 {
                return None;
            }
            let arg = try_eval(call.arguments[0].as_expression()?)?;
            let string = arg.as_str()?;
            base64_decode(string).map(JsValue::String)
        }

        // btoa(string)
        Expression::Identifier(id) if id.name.as_str() == "btoa" => {
            if call.arguments.len() != 1 {
                return None;
            }
            let arg = try_eval(call.arguments[0].as_expression()?)?;
            let string = arg.as_str()?;
            if string.chars().all(|c| (c as u32) <= 0xFF) {
                let bytes: Vec<u8> = string.chars().map(|c| c as u8).collect();
                Some(JsValue::String(base64_encode(&bytes)))
            } else {
                None
            }
        }

        // Static member calls: Number.parseInt, Number.parseFloat, String.fromCharCode
        Expression::StaticMemberExpression(member) => {
            let Expression::Identifier(object) = &member.object else {
                return None;
            };
            let method = member.property.name.as_str();
            let object_name = object.name.as_str();

            match (object_name, method) {
                ("Number", "parseInt") => {
                    if call.arguments.is_empty() || call.arguments.len() > 2 {
                        return None;
                    }
                    let arg = try_eval(call.arguments[0].as_expression()?)?;
                    let string = arg.as_str()?;
                    let radix = if call.arguments.len() == 2 {
                        let r = try_eval(call.arguments[1].as_expression()?)?.as_number()?;
                        Some(r as u32)
                    } else {
                        None
                    };
                    js_parse_int(string, radix).map(JsValue::Number)
                }
                ("Number", "parseFloat") => {
                    if call.arguments.len() != 1 {
                        return None;
                    }
                    let arg = try_eval(call.arguments[0].as_expression()?)?;
                    let string = arg.as_str()?;
                    let result: f64 = string.trim().parse().ok()?;
                    if result.is_finite() {
                        Some(JsValue::Number(result))
                    } else {
                        None
                    }
                }
                ("String", "fromCharCode") => {
                    if call.arguments.is_empty() {
                        return None;
                    }
                    let mut result = String::new();
                    for arg in &call.arguments {
                        let value = try_eval(arg.as_expression()?)?.as_number()?;
                        let code_point = value as u32;
                        result.push(char::from_u32(code_point)?);
                    }
                    Some(JsValue::String(result))
                }
                _ => None,
            }
        }

        _ => None,
    }
}

/// Conservative check: is this expression side-effect-free?
fn is_side_effect_free(expression: &Expression<'_>) -> bool {
    let expression = unwrap_parens(expression);
    matches!(
        expression,
        Expression::NumericLiteral(_)
            | Expression::StringLiteral(_)
            | Expression::BooleanLiteral(_)
            | Expression::NullLiteral(_)
    )
}

fn unwrap_parens<'a, 'b>(expression: &'b Expression<'a>) -> &'b Expression<'a> {
    let mut current = expression;
    while let Expression::ParenthesizedExpression(paren) = current {
        current = &paren.expression;
    }
    current
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxc_allocator::Allocator;
    use oxc_parser::Parser;
    use oxc_span::SourceType;

    fn eval_js(source: &str) -> Option<JsValue> {
        let allocator = Allocator::default();
        let result = Parser::new(&allocator, source, SourceType::mjs()).parse();
        assert!(!result.panicked);
        let program = result.program;
        let stmt = &program.body[0];
        if let oxc_ast::ast::Statement::ExpressionStatement(expr_stmt) = stmt {
            try_eval(&expr_stmt.expression)
        } else {
            None
        }
    }

    #[test]
    fn eval_numeric_arithmetic() {
        assert_eq!(eval_js("1 + 2"), Some(JsValue::Number(3.0)));
        assert_eq!(eval_js("10 - 3"), Some(JsValue::Number(7.0)));
        assert_eq!(eval_js("4 * 5"), Some(JsValue::Number(20.0)));
        assert_eq!(eval_js("20 / 4"), Some(JsValue::Number(5.0)));
    }

    #[test]
    fn eval_string_concat() {
        assert_eq!(
            eval_js("\"hello\" + \" world\""),
            Some(JsValue::String("hello world".to_string()))
        );
    }

    #[test]
    fn eval_unary_not() {
        assert_eq!(eval_js("!true"), Some(JsValue::Boolean(false)));
        assert_eq!(eval_js("!0"), Some(JsValue::Boolean(true)));
        assert_eq!(eval_js("!\"\""), Some(JsValue::Boolean(true)));
    }

    #[test]
    fn eval_typeof() {
        assert_eq!(
            eval_js("typeof 42"),
            Some(JsValue::String("number".to_string()))
        );
        assert_eq!(
            eval_js("typeof \"hi\""),
            Some(JsValue::String("string".to_string()))
        );
    }

    #[test]
    fn eval_parse_int() {
        assert_eq!(eval_js("parseInt(\"42\")"), Some(JsValue::Number(42.0)));
        assert_eq!(
            eval_js("parseInt(\"ff\", 16)"),
            Some(JsValue::Number(255.0))
        );
        assert_eq!(
            eval_js("parseInt(\"7901370KGklmM\")"),
            Some(JsValue::Number(7901370.0))
        );
    }

    #[test]
    fn eval_logical() {
        assert_eq!(eval_js("true && false"), Some(JsValue::Boolean(false)));
        assert_eq!(eval_js("false || true"), Some(JsValue::Boolean(true)));
        assert_eq!(eval_js("true && 42"), Some(JsValue::Number(42.0)));
    }

    #[test]
    fn eval_nested() {
        assert_eq!(eval_js("(2 + 3) * (10 - 4)"), Some(JsValue::Number(30.0)));
        assert_eq!(
            eval_js("-parseInt(\"100\") / 1 + parseInt(\"200\") / 2"),
            Some(JsValue::Number(0.0))
        );
    }

    #[test]
    fn eval_comparison() {
        assert_eq!(eval_js("1 < 2"), Some(JsValue::Boolean(true)));
        assert_eq!(eval_js("5 === 5"), Some(JsValue::Boolean(true)));
        assert_eq!(eval_js("3 !== 4"), Some(JsValue::Boolean(true)));
    }

    #[test]
    fn eval_bitwise() {
        assert_eq!(eval_js("0xFF & 0x0F"), Some(JsValue::Number(15.0)));
        assert_eq!(eval_js("1 << 4"), Some(JsValue::Number(16.0)));
        assert_eq!(eval_js("~0"), Some(JsValue::Number(-1.0)));
    }
}
