//! Transforms that evaluate known built-in functions at compile time.

mod builtin_evaluation_transformer;
mod literal_method_evaluation_transformer;

pub use builtin_evaluation_transformer::BuiltinEvaluationTransformer;
pub use literal_method_evaluation_transformer::LiteralMethodEvaluationTransformer;
