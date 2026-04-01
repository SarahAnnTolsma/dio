//! # dio-core — JavaScript Deobfuscation Library
//!
//! A high-performance JavaScript deobfuscation library that uses AST transformations
//! to simplify and clean obfuscated code.
//!
//! # Quick Start
//!
//! ```
//! use dio_core::Deobfuscator;
//!
//! let deobfuscator = Deobfuscator::new();
//! let result = deobfuscator.deobfuscate("var x = 1 + 2;");
//! // result contains cleaned JavaScript
//! ```
//!
//! # Custom Transformers
//!
//! ```ignore
//! use dio_core::{Deobfuscator, Transformer, AstNodeType};
//!
//! struct MyTransformer;
//!
//! impl Transformer for MyTransformer {
//!     fn name(&self) -> &str { "MyTransformer" }
//!     fn interests(&self) -> &[AstNodeType] { &[AstNodeType::CallExpression] }
//!     // ... implement enter_expression / exit_expression
//! }
//!
//! let mut deobfuscator = Deobfuscator::new();
//! deobfuscator.add_transformer(Box::new(MyTransformer));
//! ```

pub mod deobfuscator;
pub mod diagnostics;
pub mod operations;
pub mod pattern;
pub mod presets;
pub mod transformer;
pub mod transforms;
pub mod utils;

pub use deobfuscator::{Deobfuscator, deobfuscate};
pub use diagnostics::{TransformDiagnostics, TransformerStatistics};
pub use presets::{Preset, jsfuck_transformers, obfuscator_io_transformers};
pub use transformer::{AstNodeType, Transformer, TransformerPhase, TransformerPriority};
pub use transforms::debundler::annotate_browserify_requires;
