//! Capture types for pattern matching results.

use std::collections::HashMap;

/// The result of matching a pattern against an AST node.
#[derive(Debug, Clone)]
pub struct MatchResult {
    /// Whether the pattern matched.
    pub matched: bool,

    /// Named captures extracted during matching.
    /// Keys are the capture names provided in `Capture` patterns.
    pub captures: HashMap<String, CapturedNode>,
}

impl MatchResult {
    /// Create a successful match result with no captures.
    pub fn matched() -> Self {
        Self {
            matched: true,
            captures: HashMap::new(),
        }
    }

    /// Create a failed match result.
    pub fn no_match() -> Self {
        Self {
            matched: false,
            captures: HashMap::new(),
        }
    }

    /// Create a successful match result with the given captures.
    pub fn matched_with_captures(captures: HashMap<String, CapturedNode>) -> Self {
        Self {
            matched: true,
            captures,
        }
    }

    /// Merge captures from another match result into this one.
    /// Used internally when combining sub-pattern results.
    pub fn merge_captures(&mut self, other: &MatchResult) {
        for (key, value) in &other.captures {
            self.captures.insert(key.clone(), value.clone());
        }
    }
}

/// A value captured during pattern matching.
///
/// Captures extract concrete values from matched AST nodes, stored as
/// simple Rust types for easy access by transformers.
#[derive(Debug, Clone)]
pub enum CapturedNode {
    /// A captured string value (from identifiers or string literals).
    StringValue(String),

    /// A captured numeric value (from numeric literals).
    NumberValue(f64),

    /// A captured boolean value (from boolean literals).
    BooleanValue(bool),
}

impl CapturedNode {
    /// Try to get the captured value as a string.
    pub fn as_string(&self) -> Option<&str> {
        match self {
            CapturedNode::StringValue(value) => Some(value),
            _ => None,
        }
    }

    /// Try to get the captured value as a number.
    pub fn as_number(&self) -> Option<f64> {
        match self {
            CapturedNode::NumberValue(value) => Some(*value),
            _ => None,
        }
    }

    /// Try to get the captured value as a boolean.
    pub fn as_boolean(&self) -> Option<bool> {
        match self {
            CapturedNode::BooleanValue(value) => Some(*value),
            _ => None,
        }
    }
}
