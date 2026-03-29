//! Provides `TransformContext`, a wrapper around oxc's allocator and scoping
//! information that is threaded through the deobfuscation pipeline.

use oxc_allocator::Allocator;
use oxc_ast::AstBuilder;
use oxc_semantic::Scoping;

/// Context available to transformers during deobfuscation.
///
/// Holds references to the arena allocator (for creating new AST nodes)
/// and the current scoping information (for scope-aware transforms).
pub struct TransformContext<'a> {
    /// The arena allocator that owns all AST nodes.
    pub allocator: &'a Allocator,

    /// Current scope and binding information. Updated between iterations
    /// by re-running semantic analysis.
    pub scoping: Scoping,
}

impl<'a> TransformContext<'a> {
    /// Create a new transform context.
    pub fn new(allocator: &'a Allocator, scoping: Scoping) -> Self {
        Self { allocator, scoping }
    }

    /// Create an `AstBuilder` for constructing new AST nodes in the arena.
    pub fn ast_builder(&self) -> AstBuilder<'a> {
        AstBuilder::new(self.allocator)
    }
}
