//! Transformer preset for Akamai Bot Manager scripts.
//!
//! Extends the default transformer set with Akamai-specific transforms
//! for patterns used in Akamai's proprietary bot detection JavaScript:
//!
//! - Initializer function inlining (JSFuck constant setup, derived constants)

use crate::transformer::Transformer;
use crate::transforms;

/// Returns transformers targeting Akamai Bot Manager scripts.
pub fn transformers() -> Vec<Box<dyn Transformer>> {
    let mut result =
        vec![Box::new(transforms::akamai::InitializerInliningTransformer) as Box<dyn Transformer>];

    // Include all general-purpose transforms.
    result.extend(transforms::default_transformers());

    result
}
