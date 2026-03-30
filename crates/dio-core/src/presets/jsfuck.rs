//! Transformer preset for JSFuck-encoded JavaScript.
//!
//! JSFuck uses only `[]()!+` characters, relying on type coercion to
//! construct arbitrary JavaScript. This preset provides a focused subset
//! of transformers optimized for resolving coercion chains.

use crate::transformer::Transformer;
use crate::transforms::{constant, evaluation, simplification, string};

/// Returns transformers targeting JSFuck patterns.
pub fn transformers() -> Vec<Box<dyn Transformer>> {
    vec![
        // Constant folding handles type coercion chains:
        // ![] → false, +[] → 0, !![] → true, +!![] → 1
        Box::new(constant::ConstantFoldingTransformer),
        // String concatenation merges coerced string fragments
        Box::new(string::StringConcatenationTransformer),
        // Built-in evaluation handles Number(), Boolean(), String.fromCharCode()
        Box::new(evaluation::BuiltinEvaluationTransformer),
        // Literal method evaluation handles "string".charAt(), etc.
        Box::new(evaluation::LiteralMethodEvaluationTransformer),
        // Comma and member simplification for intermediate forms
        Box::new(simplification::CommaTransformer),
        Box::new(simplification::MemberTransformer),
    ]
}
