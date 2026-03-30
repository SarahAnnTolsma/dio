//! Inlining transforms: proxy function inlining, string array decoding, and related simplifications.

mod proxy_function_inlining_transformer;
mod string_array_decoder_transformer;

pub use proxy_function_inlining_transformer::ProxyFunctionInliningTransformer;
pub use string_array_decoder_transformer::StringArrayDecoderTransformer;
