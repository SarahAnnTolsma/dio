//! Defines the `Transformer` trait and supporting types for AST-based deobfuscation passes.

use oxc_allocator::Vec as ArenaVec;
use oxc_ast::ast::{Expression, Statement};
use oxc_traverse::TraverseCtx;

/// Specific AST node types a transformer can register interest in.
///
/// Each variant maps to a concrete oxc AST node type. Transformers declare
/// which node types they want to visit, and the dispatch visitor only calls
/// them for matching nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AstNodeType {
    // -- Expressions --
    NumericLiteral,
    StringLiteral,
    BooleanLiteral,
    NullLiteral,
    Identifier,
    BinaryExpression,
    UnaryExpression,
    LogicalExpression,
    AssignmentExpression,
    CallExpression,
    MemberExpression,
    ConditionalExpression,
    SequenceExpression,
    TemplateLiteral,
    ArrayExpression,
    ObjectExpression,
    ArrowFunctionExpression,
    FunctionExpression,

    // -- Statements --
    ExpressionStatement,
    BlockStatement,
    IfStatement,
    ReturnStatement,
    VariableDeclaration,
    ForStatement,
    ForInStatement,
    ForOfStatement,
    WhileStatement,
    DoWhileStatement,
    SwitchStatement,

    // -- Statement lists --
    /// Interest in statement lists (block bodies, program bodies, switch cases, etc.).
    /// Transformers registering for this receive `enter_statements` calls.
    StatementList,
}

/// Execution priority for transformers within the convergence loop.
///
/// Transformers with `First` priority run before `Default`, which run before `Last`.
/// Within each priority level, execution order is **not guaranteed**.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TransformerPriority {
    /// Runs before all other transformers. Use for transforms that expose
    /// opportunities for downstream transforms (e.g., constant inlining).
    First,

    /// Default priority. Most transformers use this.
    Default,

    /// Runs after all other transformers within the same phase.
    Last,
}

/// The phase in which a transformer executes.
///
/// The deobfuscator runs two phases:
/// 1. **Main** — the convergence loop where most transforms run.
/// 2. **Finalize** — runs after the main loop converges. If any finalize
///    transformer makes changes, the main loop restarts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TransformerPhase {
    /// Normal convergence loop phase.
    Main,

    /// Post-convergence pruning/cleanup phase. Transforms like dead code
    /// elimination and variable renaming typically run here.
    Finalize,
}

/// A deobfuscation transform pass.
///
/// Implementors declare which AST node types they are interested in via `interests()`,
/// and the dispatch visitor calls the appropriate `enter_*`/`exit_*` methods only
/// for matching nodes.
///
/// # Implementing a Transformer
///
/// 1. Return a human-readable name from `name()`.
/// 2. Return the `AstNodeType` variants you want to visit from `interests()`.
/// 3. Override the `enter_*` or `exit_*` methods for the node categories you handle.
/// 4. Return `true` from visit methods if you modified the AST, `false` otherwise.
///
/// Optionally override `priority()` and `phase()` to control execution ordering.
pub trait Transformer: Send + Sync {
    /// Human-readable name for logging and diagnostics.
    fn name(&self) -> &str;

    /// Which specific AST node types this transformer wants to visit.
    fn interests(&self) -> &[AstNodeType];

    /// Execution priority within the phase. Defaults to `TransformerPriority::Default`.
    fn priority(&self) -> TransformerPriority {
        TransformerPriority::Default
    }

    /// Which phase this transformer runs in. Defaults to `TransformerPhase::Main`.
    fn phase(&self) -> TransformerPhase {
        TransformerPhase::Main
    }

    /// Called when entering an expression node this transformer registered interest in.
    ///
    /// Return `true` if the expression was modified.
    fn enter_expression<'a>(
        &self,
        _expression: &mut Expression<'a>,
        _context: &mut TraverseCtx<'a, ()>,
    ) -> bool {
        false
    }

    /// Called when exiting an expression node (post-order traversal).
    ///
    /// Return `true` if the expression was modified.
    fn exit_expression<'a>(
        &self,
        _expression: &mut Expression<'a>,
        _context: &mut TraverseCtx<'a, ()>,
    ) -> bool {
        false
    }

    /// Called when entering a statement node this transformer registered interest in.
    ///
    /// Return `true` if the statement was modified.
    fn enter_statement<'a>(
        &self,
        _statement: &mut Statement<'a>,
        _context: &mut TraverseCtx<'a, ()>,
    ) -> bool {
        false
    }

    /// Called when exiting a statement node (post-order traversal).
    ///
    /// Return `true` if the statement was modified.
    fn exit_statement<'a>(
        &self,
        _statement: &mut Statement<'a>,
        _context: &mut TraverseCtx<'a, ()>,
    ) -> bool {
        false
    }

    /// Called when entering a statement list (block body, program body, etc.).
    ///
    /// Gives transformers access to the full `Vec<Statement>` for one-to-many
    /// operations like splicing or filtering. Requires registering interest in
    /// `AstNodeType::StatementList`.
    ///
    /// Return `true` if any statements were added, removed, or replaced.
    fn enter_statements<'a>(
        &self,
        _statements: &mut ArenaVec<'a, Statement<'a>>,
        _context: &mut TraverseCtx<'a, ()>,
    ) -> bool {
        false
    }
}
