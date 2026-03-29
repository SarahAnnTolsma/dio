//! Constant-related transforms: folding and inlining.

mod constant_folding_transformer;
mod constant_inlining_transformer;

pub use constant_folding_transformer::ConstantFoldingTransformer;
pub use constant_inlining_transformer::ConstantInliningTransformer;
