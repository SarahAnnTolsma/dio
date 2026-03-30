//! The main `Deobfuscator` struct and its convergence loop.

use std::collections::HashMap;

use oxc_allocator::{Allocator, Vec as ArenaVec};
use oxc_ast::ast::{Expression, Statement};
use oxc_codegen::{Codegen, CodegenOptions, IndentChar};
use oxc_parser::Parser;
use oxc_semantic::SemanticBuilder;
use oxc_span::SourceType;
use oxc_traverse::{Traverse, TraverseCtx, traverse_mut};

use crate::diagnostics::TransformDiagnostics;
use crate::transformer::{AstNodeType, Transformer, TransformerPhase, TransformerPriority};
use crate::transforms;

/// The main entry point for JavaScript deobfuscation.
///
/// Holds a list of transformers and runs them in a convergence loop until
/// no more changes are made, then runs a finalize phase for cleanup.
///
/// # Example
///
/// ```
/// use dio_core::Deobfuscator;
///
/// let deobfuscator = Deobfuscator::new();
/// let result = deobfuscator.deobfuscate("var x = 1 + 2;");
/// ```
pub struct Deobfuscator {
    /// Registered transformers, in registration order.
    transformers: Vec<Box<dyn Transformer>>,

    /// Maximum number of outer iterations (main + finalize cycles).
    max_iterations: usize,

    /// Optional callback invoked with diagnostics after deobfuscation.
    diagnostics_callback: Option<Box<dyn Fn(&TransformDiagnostics) + Send + Sync>>,

    /// Code generation options (indentation, semicolons, etc.).
    codegen_options: CodegenOptions,
}

impl Deobfuscator {
    /// Create a new deobfuscator with all built-in transformers.
    pub fn new() -> Self {
        Self {
            transformers: transforms::default_transformers(),
            max_iterations: 100,
            diagnostics_callback: None,
            codegen_options: Self::default_codegen_options(),
        }
    }

    /// Create a deobfuscator with no built-in transformers.
    /// Use `add_transformer()` to register your own.
    pub fn empty() -> Self {
        Self {
            transformers: Vec::new(),
            max_iterations: 100,
            diagnostics_callback: None,
            codegen_options: Self::default_codegen_options(),
        }
    }

    /// Default code generation options: 4-space indentation.
    fn default_codegen_options() -> CodegenOptions {
        CodegenOptions {
            indent_char: IndentChar::Space,
            indent_width: 4,
            ..CodegenOptions::default()
        }
    }

    /// Set the maximum number of outer iterations (main + finalize cycles).
    pub fn with_max_iterations(mut self, max_iterations: usize) -> Self {
        self.max_iterations = max_iterations;
        self
    }

    /// Set a callback to receive diagnostics after deobfuscation completes.
    pub fn with_diagnostics_callback<F>(mut self, callback: F) -> Self
    where
        F: Fn(&TransformDiagnostics) + Send + Sync + 'static,
    {
        self.diagnostics_callback = Some(Box::new(callback));
        self
    }

    /// Set code generation options.
    pub fn with_codegen_options(mut self, options: CodegenOptions) -> Self {
        self.codegen_options = options;
        self
    }

    /// Add a custom transformer.
    pub fn add_transformer(&mut self, transformer: Box<dyn Transformer>) {
        self.transformers.push(transformer);
    }

    /// Deobfuscate JavaScript source code.
    ///
    /// Parses the source, runs the convergence loop with all registered
    /// transformers, and returns the pretty-printed result.
    ///
    /// Returns the original source unchanged if parsing fails.
    pub fn deobfuscate(&self, source: &str) -> String {
        let allocator = Allocator::default();
        let source_type = SourceType::mjs();
        let parser_return = Parser::new(&allocator, source, source_type).parse();

        if parser_return.panicked {
            return source.to_string();
        }

        let mut program = parser_return.program;

        // Build initial scoping information (once — operations keep it in sync).
        let semantic_return = SemanticBuilder::new().build(&program);
        let mut scoping = semantic_return.semantic.into_scoping();

        // Build dispatch tables for main and finalize phases.
        let main_dispatch = DispatchTable::build(&self.transformers, TransformerPhase::Main);
        let finalize_dispatch =
            DispatchTable::build(&self.transformers, TransformerPhase::Finalize);

        // Initialize diagnostics.
        let transformer_names: Vec<&str> = self.transformers.iter().map(|t| t.name()).collect();
        let mut diagnostics = TransformDiagnostics::new(&transformer_names);

        // Outer loop: main convergence + finalize, repeat if finalize changes anything.
        for _ in 0..self.max_iterations {
            // -- Main phase: run until convergence --
            loop {
                let main_changed = {
                    let mut visitor = DispatchVisitor {
                        transformers: &self.transformers,
                        dispatch: &main_dispatch,
                        diagnostics: &mut diagnostics,
                        changed: false,
                    };

                    scoping = traverse_mut(
                        &mut visitor,
                        &allocator,
                        &mut program,
                        scoping,
                        (),
                    );

                    visitor.changed
                };

                if !main_changed {
                    break;
                }
                diagnostics.total_main_iterations += 1;
            }

            // -- Finalize phase --
            let finalize_changed = {
                let mut visitor = DispatchVisitor {
                    transformers: &self.transformers,
                    dispatch: &finalize_dispatch,
                    diagnostics: &mut diagnostics,
                    changed: false,
                };

                scoping = traverse_mut(
                    &mut visitor,
                    &allocator,
                    &mut program,
                    scoping,
                    (),
                );

                visitor.changed
            };

            diagnostics.total_finalize_iterations += 1;

            if !finalize_changed {
                // Nothing changed in finalize — we're done.
                break;
            }

            // Finalize made changes — restart main phase.
        }

        // Report diagnostics if a callback is registered.
        if let Some(callback) = &self.diagnostics_callback {
            callback(&diagnostics);
        }

        Codegen::new()
            .with_options(self.codegen_options.clone())
            .build(&program)
            .code
    }
}

impl Default for Deobfuscator {
    fn default() -> Self {
        Self::new()
    }
}

// -- Dispatch infrastructure --

/// Pre-computed mapping from `AstNodeType` to transformer indices,
/// sorted by priority within each node type.
struct DispatchTable {
    /// Maps each node type to a list of (priority, transformer_index) pairs,
    /// sorted by priority so First runs before Default before Last.
    table: HashMap<AstNodeType, Vec<usize>>,
}

impl DispatchTable {
    /// Build a dispatch table for transformers in the given phase.
    fn build(transformers: &[Box<dyn Transformer>], phase: TransformerPhase) -> Self {
        let mut table: HashMap<AstNodeType, Vec<(TransformerPriority, usize)>> = HashMap::new();

        for (index, transformer) in transformers.iter().enumerate() {
            if transformer.phase() != phase {
                continue;
            }

            let priority = transformer.priority();
            for &node_type in transformer.interests() {
                table.entry(node_type).or_default().push((priority, index));
            }
        }

        // Sort each list by priority (First < Default < Last).
        let table = table
            .into_iter()
            .map(|(node_type, mut entries)| {
                entries.sort_by_key(|(priority, _)| *priority);
                let indices: Vec<usize> = entries.into_iter().map(|(_, index)| index).collect();
                (node_type, indices)
            })
            .collect();

        Self { table }
    }

    /// Get the transformer indices interested in the given node type.
    fn get(&self, node_type: AstNodeType) -> &[usize] {
        self.table.get(&node_type).map_or(&[], |v| v.as_slice())
    }
}

/// Internal visitor that implements oxc's `Traverse` trait and dispatches
/// each node to the appropriate transformers based on the dispatch table.
struct DispatchVisitor<'t> {
    transformers: &'t [Box<dyn Transformer>],
    dispatch: &'t DispatchTable,
    diagnostics: &'t mut TransformDiagnostics,
    changed: bool,
}

impl<'t> DispatchVisitor<'t> {
    /// Dispatch an expression to all interested transformers (enter phase).
    fn dispatch_enter_expression<'a>(
        &mut self,
        node_type: AstNodeType,
        expression: &mut Expression<'a>,
        context: &mut TraverseCtx<'a, ()>,
    ) {
        for &index in self.dispatch.get(node_type) {
            self.diagnostics.record_visit(index);
            if self.transformers[index].enter_expression(expression, context) {
                self.diagnostics.record_modification(index);
                self.changed = true;
            }
        }
    }

    /// Dispatch an expression to all interested transformers (exit phase).
    fn dispatch_exit_expression<'a>(
        &mut self,
        node_type: AstNodeType,
        expression: &mut Expression<'a>,
        context: &mut TraverseCtx<'a, ()>,
    ) {
        for &index in self.dispatch.get(node_type) {
            if self.transformers[index].exit_expression(expression, context) {
                self.diagnostics.record_modification(index);
                self.changed = true;
            }
        }
    }

    /// Dispatch a statement to all interested transformers (enter phase).
    fn dispatch_enter_statement<'a>(
        &mut self,
        node_type: AstNodeType,
        statement: &mut Statement<'a>,
        context: &mut TraverseCtx<'a, ()>,
    ) {
        for &index in self.dispatch.get(node_type) {
            self.diagnostics.record_visit(index);
            if self.transformers[index].enter_statement(statement, context) {
                self.diagnostics.record_modification(index);
                self.changed = true;
            }
        }
    }

    /// Dispatch a statement to all interested transformers (exit phase).
    fn dispatch_exit_statement<'a>(
        &mut self,
        node_type: AstNodeType,
        statement: &mut Statement<'a>,
        context: &mut TraverseCtx<'a, ()>,
    ) {
        for &index in self.dispatch.get(node_type) {
            if self.transformers[index].exit_statement(statement, context) {
                self.diagnostics.record_modification(index);
                self.changed = true;
            }
        }
    }

    /// Dispatch a statement list to all interested transformers.
    fn dispatch_enter_statements<'a>(
        &mut self,
        statements: &mut ArenaVec<'a, Statement<'a>>,
        context: &mut TraverseCtx<'a, ()>,
    ) {
        for &index in self.dispatch.get(AstNodeType::StatementList) {
            self.diagnostics.record_visit(index);
            if self.transformers[index].enter_statements(statements, context) {
                self.diagnostics.record_modification(index);
                self.changed = true;
            }
        }
    }

    /// Classify an expression into its `AstNodeType`.
    fn classify_expression(expression: &Expression<'_>) -> Option<AstNodeType> {
        match expression {
            Expression::NumericLiteral(_) => Some(AstNodeType::NumericLiteral),
            Expression::StringLiteral(_) => Some(AstNodeType::StringLiteral),
            Expression::BooleanLiteral(_) => Some(AstNodeType::BooleanLiteral),
            Expression::NullLiteral(_) => Some(AstNodeType::NullLiteral),
            Expression::Identifier(_) => Some(AstNodeType::Identifier),
            Expression::BinaryExpression(_) => Some(AstNodeType::BinaryExpression),
            Expression::UnaryExpression(_) => Some(AstNodeType::UnaryExpression),
            Expression::LogicalExpression(_) => Some(AstNodeType::LogicalExpression),
            Expression::AssignmentExpression(_) => Some(AstNodeType::AssignmentExpression),
            Expression::CallExpression(_) => Some(AstNodeType::CallExpression),
            Expression::ConditionalExpression(_) => Some(AstNodeType::ConditionalExpression),
            Expression::SequenceExpression(_) => Some(AstNodeType::SequenceExpression),
            Expression::TemplateLiteral(_) => Some(AstNodeType::TemplateLiteral),
            Expression::ArrayExpression(_) => Some(AstNodeType::ArrayExpression),
            Expression::ObjectExpression(_) => Some(AstNodeType::ObjectExpression),
            Expression::ArrowFunctionExpression(_) => Some(AstNodeType::ArrowFunctionExpression),
            Expression::FunctionExpression(_) => Some(AstNodeType::FunctionExpression),
            // Member expressions are represented differently in oxc.
            Expression::StaticMemberExpression(_)
            | Expression::ComputedMemberExpression(_)
            | Expression::PrivateFieldExpression(_) => Some(AstNodeType::MemberExpression),
            _ => None,
        }
    }

    /// Classify a statement into its `AstNodeType`.
    fn classify_statement(statement: &Statement<'_>) -> Option<AstNodeType> {
        match statement {
            Statement::ExpressionStatement(_) => Some(AstNodeType::ExpressionStatement),
            Statement::BlockStatement(_) => Some(AstNodeType::BlockStatement),
            Statement::IfStatement(_) => Some(AstNodeType::IfStatement),
            Statement::ReturnStatement(_) => Some(AstNodeType::ReturnStatement),
            Statement::ForStatement(_) => Some(AstNodeType::ForStatement),
            Statement::ForInStatement(_) => Some(AstNodeType::ForInStatement),
            Statement::ForOfStatement(_) => Some(AstNodeType::ForOfStatement),
            Statement::WhileStatement(_) => Some(AstNodeType::WhileStatement),
            Statement::DoWhileStatement(_) => Some(AstNodeType::DoWhileStatement),
            Statement::SwitchStatement(_) => Some(AstNodeType::SwitchStatement),
            Statement::VariableDeclaration(_) => Some(AstNodeType::VariableDeclaration),
            _ => None,
        }
    }
}

impl<'t> Traverse<'_, ()> for DispatchVisitor<'t> {
    fn enter_expression<'a>(
        &mut self,
        expression: &mut Expression<'a>,
        context: &mut TraverseCtx<'a, ()>,
    ) {
        if let Some(node_type) = Self::classify_expression(expression) {
            self.dispatch_enter_expression(node_type, expression, context);
        }
    }

    fn exit_expression<'a>(
        &mut self,
        expression: &mut Expression<'a>,
        context: &mut TraverseCtx<'a, ()>,
    ) {
        if let Some(node_type) = Self::classify_expression(expression) {
            self.dispatch_exit_expression(node_type, expression, context);
        }
    }

    fn enter_statement<'a>(
        &mut self,
        statement: &mut Statement<'a>,
        context: &mut TraverseCtx<'a, ()>,
    ) {
        if let Some(node_type) = Self::classify_statement(statement) {
            self.dispatch_enter_statement(node_type, statement, context);
        }
    }

    fn exit_statement<'a>(
        &mut self,
        statement: &mut Statement<'a>,
        context: &mut TraverseCtx<'a, ()>,
    ) {
        if let Some(node_type) = Self::classify_statement(statement) {
            self.dispatch_exit_statement(node_type, statement, context);
        }
    }

    fn enter_statements<'a>(
        &mut self,
        statements: &mut ArenaVec<'a, Statement<'a>>,
        context: &mut TraverseCtx<'a, ()>,
    ) {
        self.dispatch_enter_statements(statements, context);
    }
}

/// Convenience free function for quick deobfuscation with default settings.
pub fn deobfuscate(source: &str) -> String {
    Deobfuscator::new().deobfuscate(source)
}
