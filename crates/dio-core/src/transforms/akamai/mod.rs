//! Transforms targeting Akamai Bot Manager scripts.
//!
//! These transformers handle obfuscation techniques specific to Akamai's
//! proprietary bot detection JavaScript. Only enabled via the `Akamai` preset.

mod initializer_inlining_transformer;

pub use initializer_inlining_transformer::InitializerInliningTransformer;
