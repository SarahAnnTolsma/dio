//! Transformer preset for debundling module bundles.
//!
//! Annotates bundled modules with JSDoc comments and named functions
//! for readability. Currently supports Browserify bundles.

use crate::transformer::Transformer;
use crate::transforms;

/// Returns transformers for debundling.
pub fn transformers() -> Vec<Box<dyn Transformer>> {
    vec![
        Box::new(transforms::debundler::BrowserifyAnnotationTransformer)
            as Box<dyn Transformer>,
    ]
}
