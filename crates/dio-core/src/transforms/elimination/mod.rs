//! Transforms that eliminate dead or unreachable code.

mod dead_code_transformer;
mod unused_variable_transformer;

pub use dead_code_transformer::DeadCodeTransformer;
pub use unused_variable_transformer::UnusedVariableTransformer;
