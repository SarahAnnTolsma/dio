//! Transforms targeting Obfuscator.io / javascript-obfuscator patterns.
//!
//! These transformers handle obfuscation techniques specific to Obfuscator.io:
//! string array decoding, string array rotation, control flow array flattening,
//! and RC4-encrypted string arrays.
//! They are only enabled via the `ObfuscatorIo` preset.

mod control_flow_array_transformer;
mod string_array_decoder_transformer;
mod string_array_rc4_decoder_transformer;
mod string_array_rotation_transformer;

pub use control_flow_array_transformer::ControlFlowArrayTransformer;
pub use string_array_decoder_transformer::StringArrayDecoderTransformer;
pub use string_array_rc4_decoder_transformer::StringArrayRC4DecoderTransformer;
pub use string_array_rotation_transformer::StringArrayRotationTransformer;
