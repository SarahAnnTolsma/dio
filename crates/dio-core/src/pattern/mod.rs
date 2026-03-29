//! Declarative pattern matching for JavaScript AST nodes.
//!
//! Provides composable, enum-based patterns that can match against oxc AST nodes
//! and optionally capture matched values.
//!
//! # Example
//!
//! ```ignore
//! use dio_core::pattern::combinators::*;
//! use oxc_syntax::operator::BinaryOperator;
//!
//! // Match: someNumber + someOtherNumber
//! let pattern = binary_expression(
//!     BinaryOperator::Addition,
//!     capture("left", any_number()),
//!     capture("right", any_number()),
//! );
//! ```

pub mod capture;
pub mod combinators;
pub mod expression;
pub mod statement;

pub use capture::{CapturedNode, MatchResult};
pub use expression::ExpressionPattern;
pub use statement::StatementPattern;
