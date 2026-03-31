//! Transforms targeting DataDome anti-bot scripts.
//!
//! These transformers handle obfuscation techniques specific to DataDome,
//! such as deferred variable assignments via `setTimeout(..., 0)`.
//! They are only enabled via the `DataDome` preset.

mod set_timeout_unwrap_transformer;

pub use set_timeout_unwrap_transformer::SetTimeoutUnwrapTransformer;
