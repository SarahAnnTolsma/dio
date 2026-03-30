//! Inlining transforms: proxy function inlining, string array decoding, and related simplifications.

mod proxy_function_inlining_transformer;
mod string_array_decoder_transformer;
mod string_array_rotation_transformer;

pub use proxy_function_inlining_transformer::ProxyFunctionInliningTransformer;
pub use string_array_decoder_transformer::StringArrayDecoderTransformer;
pub use string_array_rotation_transformer::StringArrayRotationTransformer;
