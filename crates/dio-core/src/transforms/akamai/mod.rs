//! Transforms targeting Akamai Bot Manager scripts.
//!
//! These transformers handle obfuscation techniques specific to Akamai's
//! proprietary bot detection JavaScript. Only enabled via the `Akamai` preset.

mod initializer_inlining_transformer;
#[allow(dead_code)]
mod switch_dispatch_transformer;

pub use initializer_inlining_transformer::InitializerInliningTransformer;
pub use switch_dispatch_transformer::SwitchDispatchTransformer;
