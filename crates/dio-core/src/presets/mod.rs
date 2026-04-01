//! Library-specific transformer presets for targeted deobfuscation.
//!
//! Each preset provides a curated set of transformers optimized for a
//! specific obfuscation tool or technique.
//!
//! # Usage
//!
//! ```
//! use dio_core::{Deobfuscator, Preset};
//!
//! let deobfuscator = Deobfuscator::with_preset(Preset::ObfuscatorIo);
//! let result = deobfuscator.deobfuscate("var x = 1 + 2;");
//! ```

mod datadome;
mod debundler;
mod jsfuck;
mod obfuscator_io;

use crate::transformer::Transformer;
use crate::transforms;

/// A named preset that configures the deobfuscator for a specific
/// obfuscation tool or technique.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Preset {
    /// Generic deobfuscation — the default transformer set.
    /// Handles common patterns across many obfuscation tools.
    Generic,

    /// Targets code obfuscated by Obfuscator.io / javascript-obfuscator.
    /// Includes all generic transforms plus specialized handling for
    /// string array rotation, proxy functions, and control flow flattening.
    ObfuscatorIo,

    /// Targets DataDome anti-bot scripts.
    /// Extends Obfuscator.io with DataDome-specific patterns like
    /// `setTimeout(function() { x = value; }, 0)` unwrapping.
    DataDome,

    /// Annotates bundled modules (Browserify, etc.) with JSDoc comments
    /// and named functions for readability. Can be combined with other presets.
    Debundler,

    /// Targets JSFuck-encoded JavaScript (`[]()!+` only).
    /// Focused subset: constant folding with type coercion, string
    /// concatenation, and built-in evaluation.
    JsFuck,
}

impl Preset {
    /// Returns the transformers for this preset.
    pub fn transformers(&self) -> Vec<Box<dyn Transformer>> {
        match self {
            Preset::Generic => transforms::default_transformers(),
            Preset::ObfuscatorIo => obfuscator_io::transformers(),
            Preset::DataDome => datadome::transformers(),
            Preset::Debundler => debundler::transformers(),
            Preset::JsFuck => jsfuck::transformers(),
        }
    }

    /// Parse a preset name from a string (case-insensitive, hyphen or underscore).
    pub fn from_name(name: &str) -> Option<Self> {
        match name.to_lowercase().as_str() {
            "generic" | "default" => Some(Preset::Generic),
            "obfuscator-io" | "obfuscator_io" | "javascript-obfuscator" => {
                Some(Preset::ObfuscatorIo)
            }
            "datadome" | "data-dome" | "data_dome" => Some(Preset::DataDome),
            "debundler" | "de-bundler" | "unbundler" => Some(Preset::Debundler),
            "jsfuck" => Some(Preset::JsFuck),
            _ => None,
        }
    }

    /// Returns all known preset names for help text.
    pub fn all_names() -> &'static [&'static str] {
        &["generic", "obfuscator-io", "datadome", "debundler", "jsfuck"]
    }
}

/// Returns transformers targeting Obfuscator.io / javascript-obfuscator.
pub fn obfuscator_io_transformers() -> Vec<Box<dyn Transformer>> {
    Preset::ObfuscatorIo.transformers()
}

/// Returns transformers targeting DataDome anti-bot scripts.
pub fn datadome_transformers() -> Vec<Box<dyn Transformer>> {
    Preset::DataDome.transformers()
}

/// Returns transformers for debundling (Browserify, etc.).
pub fn debundler_transformers() -> Vec<Box<dyn Transformer>> {
    Preset::Debundler.transformers()
}

/// Returns transformers targeting JSFuck-encoded JavaScript.
pub fn jsfuck_transformers() -> Vec<Box<dyn Transformer>> {
    Preset::JsFuck.transformers()
}
