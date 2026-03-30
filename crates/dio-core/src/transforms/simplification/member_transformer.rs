//! Simplifies computed member expressions to dot notation where possible.
//!
//! `obj["property"]` -> `obj.property` when the property is a valid JavaScript identifier.
//!
//! Handles both expression-position (`x["foo"]`) and assignment-target-position
//! (`x["foo"] = value`) computed member expressions.

use oxc_ast::ast::{AssignmentTarget, Expression};
use oxc_span::SPAN;
use oxc_traverse::TraverseCtx;

use crate::operations;
use crate::transformer::{AstNodeType, Transformer, TransformerPhase, TransformerPriority};

/// Converts computed member expressions with string literal keys to dot notation.
pub struct MemberTransformer;

impl Transformer for MemberTransformer {
    fn name(&self) -> &str {
        "MemberTransformer"
    }

    fn interests(&self) -> &[AstNodeType] {
        &[AstNodeType::MemberExpression, AstNodeType::AssignmentExpression]
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
        // Handle computed member in expression position: x["foo"] -> x.foo
        if let Expression::ComputedMemberExpression(_) = expression {
            return try_simplify_computed_member_expression(expression, context);
        }

        // Handle computed member on the LHS of an assignment: x["foo"] = v -> x.foo = v
        if let Expression::AssignmentExpression(assignment) = expression {
            let AssignmentTarget::ComputedMemberExpression(computed) = &assignment.left else {
                return false;
            };

            let Expression::StringLiteral(property_literal) = &computed.expression else {
                return false;
            };

            let property_name = property_literal.value.as_str();
            if !is_valid_javascript_identifier(property_name) {
                return false;
            }

            let property_identifier = context.ast.identifier_name(SPAN, property_name);

            // Re-borrow mutably to perform the replacement.
            let Expression::AssignmentExpression(assignment) = expression else {
                return false;
            };
            let AssignmentTarget::ComputedMemberExpression(computed) = &mut assignment.left else {
                return false;
            };

            let object = std::mem::replace(
                &mut computed.object,
                context.ast.expression_null_literal(SPAN),
            );
            let optional = computed.optional;

            let static_member = context.ast.alloc_static_member_expression(
                SPAN,
                object,
                property_identifier,
                optional,
            );
            assignment.left = AssignmentTarget::StaticMemberExpression(static_member);
            return true;
        }

        false
    }
}

/// Simplify a computed member expression in expression position.
fn try_simplify_computed_member_expression<'a>(
    expression: &mut Expression<'a>,
    context: &mut TraverseCtx<'a, ()>,
) -> bool {
    let Expression::ComputedMemberExpression(computed) = expression else {
        return false;
    };

    let Expression::StringLiteral(property_literal) = &computed.expression else {
        return false;
    };

    let property_name = property_literal.value.as_str();

    if !is_valid_javascript_identifier(property_name) {
        return false;
    }

    let property_identifier = context.ast.identifier_name(SPAN, property_name);

    // Take the object out, replacing with a dummy.
    let object = std::mem::replace(
        &mut computed.object,
        context.ast.expression_null_literal(SPAN),
    );
    let optional = computed.optional;

    let static_member =
        context
            .ast
            .alloc_static_member_expression(SPAN, object, property_identifier, optional);
    let replacement = Expression::StaticMemberExpression(static_member);
    operations::replace_expression(expression, replacement, context);
    true
}

/// Check if a string is a valid JavaScript identifier (simplified).
fn is_valid_javascript_identifier(name: &str) -> bool {
    if name.is_empty() || is_javascript_reserved_word(name) {
        return false;
    }

    let mut characters = name.chars();
    let first = characters.next().unwrap();
    if !first.is_ascii_alphabetic() && first != '_' && first != '$' {
        return false;
    }

    characters.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
}

fn is_javascript_reserved_word(name: &str) -> bool {
    matches!(
        name,
        "break"
            | "case"
            | "catch"
            | "continue"
            | "debugger"
            | "default"
            | "delete"
            | "do"
            | "else"
            | "finally"
            | "for"
            | "function"
            | "if"
            | "in"
            | "instanceof"
            | "new"
            | "return"
            | "switch"
            | "this"
            | "throw"
            | "try"
            | "typeof"
            | "var"
            | "void"
            | "while"
            | "with"
            | "class"
            | "const"
            | "enum"
            | "export"
            | "extends"
            | "import"
            | "super"
            | "implements"
            | "interface"
            | "let"
            | "package"
            | "private"
            | "protected"
            | "public"
            | "static"
            | "yield"
    )
}
