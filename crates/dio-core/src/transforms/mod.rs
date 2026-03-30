//! Built-in transformer implementations grouped by category.

pub mod constant;
pub mod elimination;
pub mod evaluation;
pub mod inlining;
pub mod renaming;
pub mod simplification;
pub mod string;

use crate::transformer::Transformer;

/// Returns the default set of built-in transformers.
///
/// Registration order does not matter — the dispatch system groups
/// transformers by phase and priority, and execution order within the
/// same priority level is not guaranteed.
pub fn default_transformers() -> Vec<Box<dyn Transformer>> {
    vec![
        // Main phase, First priority
        Box::new(simplification::VariableDeclarationSplitTransformer),
        Box::new(constant::ConstantInliningTransformer::default()),
        Box::new(inlining::ProxyFunctionInliningTransformer::default()),
        // Main phase, Default priority
        Box::new(simplification::BlockNormalizationTransformer),
        Box::new(simplification::BitwiseSimplificationTransformer),
        Box::new(constant::ConstantFoldingTransformer),
        Box::new(string::StringConcatenationTransformer),
        Box::new(evaluation::BuiltinEvaluationTransformer),
        Box::new(evaluation::LiteralMethodEvaluationTransformer),
        Box::new(simplification::CommaTransformer),
        Box::new(simplification::MemberTransformer),
        Box::new(simplification::ControlFlowTransformer),
        Box::new(simplification::TernaryToIfTransformer),
        Box::new(simplification::LogicalToIfTransformer),
        Box::new(simplification::SequenceStatementTransformer),
        // Finalize phase
        Box::new(elimination::DeadCodeTransformer),
        Box::new(elimination::UnusedVariableTransformer),
        Box::new(renaming::VariableRenamingTransformer),
    ]
}
