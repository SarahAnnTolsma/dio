//! Transforms for debundling module bundles (Browserify, etc.).
//!
//! These transformers annotate bundled modules with JSDoc comments and
//! named functions to improve readability. Only enabled via the `Debundler` preset.

mod browserify_annotation_transformer;

pub use browserify_annotation_transformer::BrowserifyAnnotationTransformer;
