//! Transformer preset for JSFuck-encoded JavaScript.
//!
//! JSFuck uses only `[]()!+` characters, relying on type coercion to
//! construct arbitrary JavaScript. This preset includes all general-purpose
//! transformers — the constant folding transformer already handles JSFuck
//! coercion chains (`![] → false`, `+[] → 0`, `!![] → true`, `+!![] → 1`).

use crate::transformer::Transformer;
use crate::transforms;

/// Returns transformers targeting JSFuck patterns.
///
/// Includes all default transformers since JSFuck-encoded code benefits
/// from the full pipeline (constant folding, string concatenation, dead
/// code elimination, etc.) after the coercion chains are resolved.
pub fn transformers() -> Vec<Box<dyn Transformer>> {
    transforms::default_transformers()
}
