//! Inlines constants that are assigned once and never reassigned.
//!
//! Example: `const x = 5; f(x);` -> `f(5);`
//!
//! This is a scope-aware transform that uses oxc's semantic analysis to find
//! variables with a single constant initializer and replace all references.
//!
//! NOTE: This is a placeholder implementation. Full constant inlining requires
//! walking variable declarations and their references via scoping, which needs
//! a full-program pass rather than per-node dispatch. For now, this is a stub
//! that will be fleshed out once the core pipeline is validated.

use crate::transformer::{AstNodeType, Transformer, TransformerPhase, TransformerPriority};

/// Inlines constant variables that are assigned once to a literal value.
pub struct ConstantInliningTransformer;

impl Transformer for ConstantInliningTransformer {
    fn name(&self) -> &str {
        "ConstantInliningTransformer"
    }

    fn interests(&self) -> &[AstNodeType] {
        // Interested in variable declarations to find single-assignment constants.
        &[AstNodeType::VariableDeclaration]
    }

    fn priority(&self) -> TransformerPriority {
        TransformerPriority::First
    }

    fn phase(&self) -> TransformerPhase {
        TransformerPhase::Main
    }

    // TODO: Implement full constant inlining using scoping information.
    // This requires:
    // 1. Finding `const` declarations with literal initializers.
    // 2. Using scoping to find all references to the binding.
    // 3. Replacing each reference with a copy of the literal.
    // 4. Optionally removing the now-unused declaration.
}
