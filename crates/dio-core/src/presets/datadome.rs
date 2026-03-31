//! Transformer preset for DataDome anti-bot scripts.
//!
//! Extends the Obfuscator.io preset with additional transforms specific
//! to DataDome's obfuscation patterns:
//!
//! - `setTimeout(function() { x = value; }, 0)` unwrapping

use crate::transformer::Transformer;
use crate::transforms;

use super::obfuscator_io;

/// Returns transformers targeting DataDome scripts.
pub fn transformers() -> Vec<Box<dyn Transformer>> {
    let mut result = Vec::new();

    // DataDome-specific transforms.
    result.push(
        Box::new(transforms::datadome::SetTimeoutUnwrapTransformer) as Box<dyn Transformer>,
    );

    // Include all Obfuscator.io transforms.
    result.extend(obfuscator_io::transformers());

    result
}
