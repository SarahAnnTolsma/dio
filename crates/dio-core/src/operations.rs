//! Scope-aware AST mutation operations.
//!
//! Provides functions that replace, remove, or rename AST nodes while keeping
//! oxc's `Scoping` data (references, bindings, symbols) in sync. Transformers
//! must use these functions instead of directly assigning to AST node references.

use std::collections::HashSet;

use oxc_allocator::Vec as ArenaVec;
use oxc_ast::ast::{Expression, IdentifierReference, Statement};
use oxc_ast_visit::{Visit, walk};
use oxc_span::{Ident, SPAN};
use oxc_syntax::reference::ReferenceId;
use oxc_syntax::symbol::SymbolId;
use oxc_traverse::TraverseCtx;

// ---------------------------------------------------------------------------
// One-to-one operations
// ---------------------------------------------------------------------------

/// Replace an expression, cleaning up orphaned identifier references from the
/// old subtree that are not present in the new subtree.
pub fn replace_expression<'a>(
    target: &mut Expression<'a>,
    replacement: Expression<'a>,
    context: &mut TraverseCtx<'a, ()>,
) {
    let old_references = collect_expression_references(target);
    let new_references = collect_expression_references(&replacement);
    *target = replacement;
    delete_orphaned_references(&old_references, &new_references, context);
}

/// Replace a statement, cleaning up orphaned identifier references from the
/// old subtree that are not present in the new subtree.
pub fn replace_statement<'a>(
    target: &mut Statement<'a>,
    replacement: Statement<'a>,
    context: &mut TraverseCtx<'a, ()>,
) {
    let old_references = collect_statement_references(target);
    let new_references = collect_statement_references(&replacement);
    *target = replacement;
    delete_orphaned_references(&old_references, &new_references, context);
}

/// Remove an expression by replacing it with `void 0` (`undefined`),
/// cleaning up all identifier references in the removed subtree.
pub fn remove_expression<'a>(
    target: &mut Expression<'a>,
    context: &mut TraverseCtx<'a, ()>,
) {
    let old_references = collect_expression_references(target);
    *target = context.ast.void_0(SPAN);
    delete_all_references(&old_references, context);
}

/// Remove a statement by replacing it with an empty statement,
/// cleaning up all identifier references in the removed subtree.
pub fn remove_statement<'a>(
    target: &mut Statement<'a>,
    context: &mut TraverseCtx<'a, ()>,
) {
    let old_references = collect_statement_references(target);
    *target = context.ast.statement_empty(SPAN);
    delete_all_references(&old_references, context);
}

/// Rename a binding in scoping data (symbol table and bindings map).
///
/// This updates the symbol name and the scope's binding entry. It does **not**
/// update AST nodes (`IdentifierReference.name`, `BindingIdentifier.name`) —
/// the caller is responsible for patching those during traversal.
pub fn rename_binding<'a>(
    symbol_id: SymbolId,
    new_name: Ident<'a>,
    context: &mut TraverseCtx<'a, ()>,
) {
    let scope_id = context.scoping().symbol_scope_id(symbol_id);
    context
        .scoping_mut()
        .rename_symbol(symbol_id, scope_id, new_name);
}

// ---------------------------------------------------------------------------
// One-to-many operations
// ---------------------------------------------------------------------------

/// Replace a statement at `index` in a statement list with multiple statements.
///
/// Cleans up references in the removed statement. The replacement statements'
/// references are assumed to already be registered in scoping (e.g. moved from
/// the original or freshly created by the caller).
pub fn replace_statement_with_multiple<'a>(
    statements: &mut ArenaVec<'a, Statement<'a>>,
    index: usize,
    replacements: ArenaVec<'a, Statement<'a>>,
    context: &mut TraverseCtx<'a, ()>,
) {
    let old_references = collect_statement_references(&statements[index]);
    let mut new_references = Vec::new();
    for replacement in &replacements {
        let mut collector = ReferenceCollector::new();
        collector.visit_statement(replacement);
        new_references.extend(collector.references);
    }
    let replacement_count = replacements.len();
    // Splice: remove the element at index, insert replacements.
    // oxc's arena Vec doesn't have splice, so we do it manually.
    // First, replace the element at index with the first replacement (or empty if none).
    if replacement_count == 0 {
        statements[index] = context.ast.statement_empty(SPAN);
    } else {
        let mut replacements_iter = replacements.into_iter();
        statements[index] = replacements_iter.next().unwrap();
        // Insert remaining replacements after index.
        let mut insert_position = index + 1;
        for replacement in replacements_iter {
            statements.insert(insert_position, replacement);
            insert_position += 1;
        }
    }
    delete_orphaned_references(&old_references, &new_references, context);
}

/// Remove a statement at `index` from a statement list by replacing it with
/// an empty statement. Cleans up all references in the removed statement.
pub fn remove_statement_at<'a>(
    statements: &mut ArenaVec<'a, Statement<'a>>,
    index: usize,
    context: &mut TraverseCtx<'a, ()>,
) {
    let old_references = collect_statement_references(&statements[index]);
    statements[index] = context.ast.statement_empty(SPAN);
    delete_all_references(&old_references, context);
}

/// Remove statements from a list that do not satisfy the predicate.
/// Cleans up references in all removed statements.
pub fn retain_statements<'a, F>(
    statements: &mut ArenaVec<'a, Statement<'a>>,
    mut predicate: F,
    context: &mut TraverseCtx<'a, ()>,
) where
    F: FnMut(&Statement<'a>) -> bool,
{
    for index in 0..statements.len() {
        if !predicate(&statements[index]) {
            let old_references = collect_statement_references(&statements[index]);
            statements[index] = context.ast.statement_empty(SPAN);
            delete_all_references(&old_references, context);
        }
    }
}

/// Insert a statement into a statement list at the given index.
///
/// The statement's references are assumed to already be registered in scoping
/// (e.g. created via `context.create_bound_ident_reference`).
pub fn insert_statement<'a>(
    statements: &mut ArenaVec<'a, Statement<'a>>,
    index: usize,
    statement: Statement<'a>,
) {
    statements.insert(index, statement);
}

/// Append a statement to the end of a statement list.
///
/// The statement's references are assumed to already be registered in scoping.
pub fn append_statement<'a>(
    statements: &mut ArenaVec<'a, Statement<'a>>,
    statement: Statement<'a>,
) {
    statements.push(statement);
}

/// Insert an expression into an expression list at the given index.
///
/// Works with `SequenceExpression.expressions`, `ArrayExpression.elements`,
/// `CallExpression.arguments`, etc. The expression's references are assumed
/// to already be registered in scoping.
pub fn insert_expression<'a>(
    expressions: &mut ArenaVec<'a, Expression<'a>>,
    index: usize,
    expression: Expression<'a>,
) {
    expressions.insert(index, expression);
}

/// Append an expression to the end of an expression list.
///
/// The expression's references are assumed to already be registered in scoping.
pub fn append_expression<'a>(
    expressions: &mut ArenaVec<'a, Expression<'a>>,
    expression: Expression<'a>,
) {
    expressions.push(expression);
}

/// Replace an expression with a sequence expression wrapping multiple expressions.
///
/// Cleans up references from the old expression that are not present in
/// the new sequence.
pub fn replace_expression_with_sequence<'a>(
    target: &mut Expression<'a>,
    expressions: ArenaVec<'a, Expression<'a>>,
    context: &mut TraverseCtx<'a, ()>,
) {
    let old_references = collect_expression_references(target);
    let mut new_references = Vec::new();
    for expression in &expressions {
        let mut collector = ReferenceCollector::new();
        collector.visit_expression(expression);
        new_references.extend(collector.references);
    }
    *target = context.ast.expression_sequence(SPAN, expressions);
    delete_orphaned_references(&old_references, &new_references, context);
}

// ---------------------------------------------------------------------------
// Exposed helpers
// ---------------------------------------------------------------------------

/// Collect all `(ReferenceId, Ident)` pairs from `IdentifierReference` nodes
/// in an expression subtree. Descends into nested function/arrow scopes.
pub fn collect_expression_references<'a>(
    expression: &Expression<'a>,
) -> Vec<(ReferenceId, Ident<'a>)> {
    let mut collector = ReferenceCollector::new();
    collector.visit_expression(expression);
    collector.references
}

/// Collect all `(ReferenceId, Ident)` pairs from `IdentifierReference` nodes
/// in a statement subtree. Descends into nested function/arrow scopes.
pub fn collect_statement_references<'a>(
    statement: &Statement<'a>,
) -> Vec<(ReferenceId, Ident<'a>)> {
    let mut collector = ReferenceCollector::new();
    collector.visit_statement(statement);
    collector.references
}

/// Delete a batch of references from scoping.
pub fn delete_references(
    references: &[(ReferenceId, Ident<'_>)],
    context: &mut TraverseCtx<'_, ()>,
) {
    for &(reference_id, name) in references {
        context.delete_reference(reference_id, name);
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Delete references that appear in `old` but not in `new`.
fn delete_orphaned_references(
    old_references: &[(ReferenceId, Ident<'_>)],
    new_references: &[(ReferenceId, Ident<'_>)],
    context: &mut TraverseCtx<'_, ()>,
) {
    if old_references.is_empty() {
        return;
    }
    let new_ids: HashSet<ReferenceId> = new_references.iter().map(|&(id, _)| id).collect();
    for &(reference_id, name) in old_references {
        if !new_ids.contains(&reference_id) {
            context.delete_reference(reference_id, name);
        }
    }
}

/// Delete all references in the list from scoping.
fn delete_all_references(
    references: &[(ReferenceId, Ident<'_>)],
    context: &mut TraverseCtx<'_, ()>,
) {
    for &(reference_id, name) in references {
        context.delete_reference(reference_id, name);
    }
}

/// Walks an AST subtree and collects all `IdentifierReference` nodes'
/// `(ReferenceId, Ident)` pairs. Descends into all scopes including
/// nested functions and arrows.
struct ReferenceCollector<'a> {
    references: Vec<(ReferenceId, Ident<'a>)>,
}

impl<'a> ReferenceCollector<'a> {
    fn new() -> Self {
        Self {
            references: Vec::new(),
        }
    }
}

impl<'a> Visit<'a> for ReferenceCollector<'a> {
    fn visit_identifier_reference(&mut self, identifier: &IdentifierReference<'a>) {
        if let Some(reference_id) = identifier.reference_id.get() {
            self.references.push((reference_id, identifier.name));
        }
        walk::walk_identifier_reference(self, identifier);
    }

    // We intentionally use the default implementations for visit_function and
    // visit_arrow_function_expression, which DO descend into their bodies.
    // This ensures all references in discarded subtrees are cleaned up.
}

#[cfg(test)]
mod tests {
    use oxc_allocator::Allocator;
    use oxc_ast::ast::{Expression, Statement};
    use oxc_parser::Parser;
    use oxc_semantic::{Scoping, SemanticBuilder};
    use oxc_span::{SPAN, SourceType};
    use oxc_syntax::number::NumberBase;
    use oxc_syntax::reference::ReferenceFlags;
    use oxc_syntax::symbol::SymbolId;
    use oxc_traverse::{Traverse, TraverseCtx, traverse_mut};

    use super::*;

    /// Parse JS source and build scoping. Returns (program, scoping).
    /// The program lives in the allocator.
    fn parse_and_analyze<'a>(
        allocator: &'a Allocator,
        source: &'a str,
    ) -> (oxc_ast::ast::Program<'a>, Scoping) {
        let parser_return = Parser::new(allocator, source, SourceType::mjs()).parse();
        assert!(!parser_return.panicked, "Parse failed");
        let program = parser_return.program;
        let semantic_return = SemanticBuilder::new().build(&program);
        let scoping = semantic_return.semantic.into_scoping();
        (program, scoping)
    }

    /// Find the SymbolId for a binding with the given name.
    fn find_symbol(scoping: &Scoping, name: &str) -> Option<SymbolId> {
        scoping.symbol_ids().find(|&id| scoping.symbol_name(id) == name)
    }

    /// Count the number of resolved references to a symbol.
    fn reference_count(scoping: &Scoping, symbol_id: SymbolId) -> usize {
        scoping.get_resolved_reference_ids(symbol_id).len()
    }

    // -----------------------------------------------------------------------
    // replace_expression tests
    // -----------------------------------------------------------------------

    /// Visitor that replaces call expressions (e.g., `parseInt("10")`) with
    /// a numeric literal, testing that the callee identifier reference is cleaned up.
    struct ReplaceCallWithLiteral;

    impl<'a> Traverse<'a, ()> for ReplaceCallWithLiteral {
        fn enter_expression(
            &mut self,
            expression: &mut Expression<'a>,
            context: &mut TraverseCtx<'a, ()>,
        ) {
            if matches!(expression, Expression::CallExpression(_)) {
                let raw = context.ast.atom("42");
                let replacement = context
                    .ast
                    .expression_numeric_literal(SPAN, 42.0, Some(raw), NumberBase::Decimal);
                replace_expression(expression, replacement, context);
            }
        }
    }

    #[test]
    fn replace_expression_cleans_up_callee_reference() {
        // `parseInt` is an unresolved global reference.
        // `x` is a resolved reference to the binding `x`.
        // After replacing `parseInt(x)` with `42`, both references should be gone.
        let allocator = Allocator::default();
        let (mut program, scoping) = parse_and_analyze(&allocator, "let x = 1; parseInt(x);");

        let x_symbol = find_symbol(&scoping, "x").expect("should find symbol x");
        assert_eq!(reference_count(&scoping, x_symbol), 1, "x should have 1 reference before");

        let mut visitor = ReplaceCallWithLiteral;
        let scoping = traverse_mut(&mut visitor, &allocator, &mut program, scoping, ());

        assert_eq!(reference_count(&scoping, x_symbol), 0, "x should have 0 references after");
    }

    /// Visitor that replaces a sequence expression `(1, 2, x)` with its last
    /// element, testing that the moved reference is preserved.
    struct ReplaceSequenceWithLast;

    impl<'a> Traverse<'a, ()> for ReplaceSequenceWithLast {
        fn enter_expression(
            &mut self,
            expression: &mut Expression<'a>,
            context: &mut TraverseCtx<'a, ()>,
        ) {
            let Expression::SequenceExpression(sequence) = expression else {
                return;
            };
            if sequence.expressions.len() > 1 {
                let last = sequence.expressions.pop().unwrap();
                replace_expression(expression, last, context);
            }
        }
    }

    #[test]
    fn replace_expression_preserves_moved_child_reference() {
        // `(1, 2, x)` -> `x`: the reference to x should be preserved.
        let allocator = Allocator::default();
        let (mut program, scoping) = parse_and_analyze(&allocator, "let x = 1; (1, 2, x);");

        let x_symbol = find_symbol(&scoping, "x").expect("should find symbol x");
        assert_eq!(reference_count(&scoping, x_symbol), 1, "x should have 1 reference before");

        let mut visitor = ReplaceSequenceWithLast;
        let scoping = traverse_mut(&mut visitor, &allocator, &mut program, scoping, ());

        assert_eq!(reference_count(&scoping, x_symbol), 1, "x should still have 1 reference after");
    }

    /// Visitor that replaces a conditional expression `cond ? a : b` with the
    /// consequent, testing that the alternate branch's references are cleaned up.
    struct ReplaceConditionalWithConsequent;

    impl<'a> Traverse<'a, ()> for ReplaceConditionalWithConsequent {
        fn enter_expression(
            &mut self,
            expression: &mut Expression<'a>,
            context: &mut TraverseCtx<'a, ()>,
        ) {
            let Expression::ConditionalExpression(conditional) = expression else {
                return;
            };
            let consequent = std::mem::replace(
                &mut conditional.consequent,
                context.ast.expression_null_literal(SPAN),
            );
            replace_expression(expression, consequent, context);
        }
    }

    #[test]
    fn replace_expression_cleans_up_discarded_branch() {
        // `true ? a : b` -> `a`: reference to `b` should be cleaned up,
        // reference to `a` should be preserved.
        let allocator = Allocator::default();
        let (mut program, scoping) =
            parse_and_analyze(&allocator, "let a = 1; let b = 2; true ? a : b;");

        let a_symbol = find_symbol(&scoping, "a").expect("should find symbol a");
        let b_symbol = find_symbol(&scoping, "b").expect("should find symbol b");
        assert_eq!(reference_count(&scoping, a_symbol), 1);
        assert_eq!(reference_count(&scoping, b_symbol), 1);

        let mut visitor = ReplaceConditionalWithConsequent;
        let scoping = traverse_mut(&mut visitor, &allocator, &mut program, scoping, ());

        assert_eq!(reference_count(&scoping, a_symbol), 1, "a ref should be preserved");
        assert_eq!(reference_count(&scoping, b_symbol), 0, "b ref should be cleaned up");
    }

    // -----------------------------------------------------------------------
    // remove_expression tests
    // -----------------------------------------------------------------------

    /// Visitor that removes the first identifier expression it finds.
    struct RemoveIdentifierExpression;

    impl<'a> Traverse<'a, ()> for RemoveIdentifierExpression {
        fn enter_expression(
            &mut self,
            expression: &mut Expression<'a>,
            context: &mut TraverseCtx<'a, ()>,
        ) {
            if matches!(expression, Expression::Identifier(_)) {
                remove_expression(expression, context);
            }
        }
    }

    #[test]
    fn remove_expression_cleans_up_reference() {
        let allocator = Allocator::default();
        let (mut program, scoping) = parse_and_analyze(&allocator, "let x = 1; x;");

        let x_symbol = find_symbol(&scoping, "x").expect("should find symbol x");
        assert_eq!(reference_count(&scoping, x_symbol), 1);

        let mut visitor = RemoveIdentifierExpression;
        let scoping = traverse_mut(&mut visitor, &allocator, &mut program, scoping, ());

        assert_eq!(reference_count(&scoping, x_symbol), 0, "x ref should be removed");
    }

    // -----------------------------------------------------------------------
    // remove_statement tests
    // -----------------------------------------------------------------------

    /// Visitor that removes expression statements containing call expressions.
    struct RemoveCallStatements;

    impl<'a> Traverse<'a, ()> for RemoveCallStatements {
        fn enter_statement(
            &mut self,
            statement: &mut Statement<'a>,
            context: &mut TraverseCtx<'a, ()>,
        ) {
            if let Statement::ExpressionStatement(expression_statement) = statement {
                if matches!(&expression_statement.expression, Expression::CallExpression(_)) {
                    remove_statement(statement, context);
                }
            }
        }
    }

    #[test]
    fn remove_statement_cleans_up_all_references() {
        // `foo(a, b)` contains references to `foo`, `a`, `b`.
        // Removing the statement should clean up all of them.
        let allocator = Allocator::default();
        let (mut program, scoping) =
            parse_and_analyze(&allocator, "let a = 1; let b = 2; function foo() {} foo(a, b);");

        let a_symbol = find_symbol(&scoping, "a").expect("should find symbol a");
        let b_symbol = find_symbol(&scoping, "b").expect("should find symbol b");
        let foo_symbol = find_symbol(&scoping, "foo").expect("should find symbol foo");
        assert_eq!(reference_count(&scoping, a_symbol), 1);
        assert_eq!(reference_count(&scoping, b_symbol), 1);
        assert_eq!(reference_count(&scoping, foo_symbol), 1);

        let mut visitor = RemoveCallStatements;
        let scoping = traverse_mut(&mut visitor, &allocator, &mut program, scoping, ());

        assert_eq!(reference_count(&scoping, a_symbol), 0);
        assert_eq!(reference_count(&scoping, b_symbol), 0);
        assert_eq!(reference_count(&scoping, foo_symbol), 0);
    }

    // -----------------------------------------------------------------------
    // replace_statement tests
    // -----------------------------------------------------------------------

    /// Visitor that replaces `if (true) A else B` with A.
    struct ReplaceIfWithConsequent;

    impl<'a> Traverse<'a, ()> for ReplaceIfWithConsequent {
        fn enter_statement(
            &mut self,
            statement: &mut Statement<'a>,
            context: &mut TraverseCtx<'a, ()>,
        ) {
            let Statement::IfStatement(if_stmt) = statement else {
                return;
            };
            let consequent = std::mem::replace(
                &mut if_stmt.consequent,
                context.ast.statement_empty(SPAN),
            );
            replace_statement(statement, consequent, context);
        }
    }

    #[test]
    fn replace_statement_cleans_up_alternate_branch() {
        // `if (true) a; else b;` -> `a;`
        // Reference to `b` should be cleaned up, `a` preserved.
        let allocator = Allocator::default();
        let (mut program, scoping) =
            parse_and_analyze(&allocator, "let a = 1; let b = 2; if (true) a; else b;");

        let a_symbol = find_symbol(&scoping, "a").expect("should find symbol a");
        let b_symbol = find_symbol(&scoping, "b").expect("should find symbol b");
        assert_eq!(reference_count(&scoping, a_symbol), 1);
        assert_eq!(reference_count(&scoping, b_symbol), 1);

        let mut visitor = ReplaceIfWithConsequent;
        let scoping = traverse_mut(&mut visitor, &allocator, &mut program, scoping, ());

        assert_eq!(reference_count(&scoping, a_symbol), 1, "a ref should be preserved");
        assert_eq!(reference_count(&scoping, b_symbol), 0, "b ref should be cleaned up");
    }

    // -----------------------------------------------------------------------
    // rename_binding tests
    // -----------------------------------------------------------------------

    /// Visitor that renames all bindings named `_0x1` to `renamed`.
    struct RenameObfuscatedBinding;

    impl<'a> Traverse<'a, ()> for RenameObfuscatedBinding {
        fn enter_expression(
            &mut self,
            expression: &mut Expression<'a>,
            context: &mut TraverseCtx<'a, ()>,
        ) {
            let Expression::Identifier(identifier) = expression else {
                return;
            };
            let Some(reference_id) = identifier.reference_id.get() else {
                return;
            };
            let reference = context.scoping().get_reference(reference_id);
            let Some(symbol_id) = reference.symbol_id() else {
                return;
            };
            if context.scoping().symbol_name(symbol_id) == "_0x1" {
                let new_name: oxc_span::Ident<'a> = context.ast.atom("renamed").into();
                rename_binding(symbol_id, new_name, context);
            }
        }
    }

    #[test]
    fn rename_binding_updates_scoping() {
        let allocator = Allocator::default();
        let (mut program, scoping) = parse_and_analyze(&allocator, "let _0x1 = 42; _0x1;");

        let symbol = find_symbol(&scoping, "_0x1").expect("should find symbol _0x1");
        assert_eq!(reference_count(&scoping, symbol), 1);

        let mut visitor = RenameObfuscatedBinding;
        let scoping = traverse_mut(&mut visitor, &allocator, &mut program, scoping, ());

        // Symbol should now be named "renamed" in scoping.
        assert_eq!(scoping.symbol_name(symbol), "renamed");
        // References should still be intact (rename_binding doesn't touch ref count).
        assert_eq!(reference_count(&scoping, symbol), 1);
        // Old name should no longer resolve.
        assert!(find_symbol(&scoping, "_0x1").is_none(), "old name should not resolve");
    }

    // -----------------------------------------------------------------------
    // collect_expression_references tests
    // -----------------------------------------------------------------------

    #[test]
    fn collect_references_finds_nested_identifiers() {
        // `a + b` should collect references to both `a` and `b`.
        let allocator = Allocator::default();
        let (program, _scoping) = parse_and_analyze(&allocator, "let a = 1; let b = 2; a + b;");

        // Find the expression statement `a + b`
        let expression_statement = program.body.iter().find_map(|stmt| {
            if let Statement::ExpressionStatement(expr_stmt) = stmt {
                if matches!(&expr_stmt.expression, Expression::BinaryExpression(_)) {
                    return Some(&expr_stmt.expression);
                }
            }
            None
        });
        let expression = expression_statement.expect("should find binary expression");
        let refs = collect_expression_references(expression);
        assert_eq!(refs.len(), 2, "should collect 2 references from `a + b`");
    }

    #[test]
    fn collect_references_from_literal_is_empty() {
        let allocator = Allocator::default();
        let (program, _scoping) = parse_and_analyze(&allocator, "42;");

        let expression_statement = program.body.iter().find_map(|stmt| {
            if let Statement::ExpressionStatement(expr_stmt) = stmt {
                return Some(&expr_stmt.expression);
            }
            None
        });
        let expression = expression_statement.expect("should find expression");
        let refs = collect_expression_references(expression);
        assert!(refs.is_empty(), "literal should have no references");
    }

    #[test]
    fn collect_references_descends_into_nested_functions() {
        // `function() { return x; }` should collect the reference to `x`
        // inside the function body.
        let allocator = Allocator::default();
        let (program, _scoping) =
            parse_and_analyze(&allocator, "let x = 1; (function() { return x; });");

        // Find the function expression
        let expression_statement = program.body.iter().find_map(|stmt| {
            if let Statement::ExpressionStatement(expr_stmt) = stmt {
                if matches!(
                    &expr_stmt.expression,
                    Expression::ParenthesizedExpression(_)
                        | Expression::FunctionExpression(_)
                ) {
                    return Some(&expr_stmt.expression);
                }
            }
            None
        });
        let expression = expression_statement.expect("should find function expression");
        let refs = collect_expression_references(expression);
        assert!(
            !refs.is_empty(),
            "should find references inside nested function"
        );
    }

    // -----------------------------------------------------------------------
    // remove_statement_at / retain_statements tests
    // -----------------------------------------------------------------------

    /// Visitor that uses enter_statements to remove statements by index.
    struct RemoveSecondStatement;

    impl<'a> Traverse<'a, ()> for RemoveSecondStatement {
        fn enter_statements(
            &mut self,
            statements: &mut ArenaVec<'a, Statement<'a>>,
            context: &mut TraverseCtx<'a, ()>,
        ) {
            if statements.len() >= 3 {
                // Remove the third statement (index 2), which should be `b;`
                remove_statement_at(statements, 2, context);
            }
        }
    }

    #[test]
    fn remove_statement_at_cleans_up_references() {
        let allocator = Allocator::default();
        let (mut program, scoping) =
            parse_and_analyze(&allocator, "let a = 1; let b = 2; b; a;");

        let a_symbol = find_symbol(&scoping, "a").expect("should find symbol a");
        let b_symbol = find_symbol(&scoping, "b").expect("should find symbol b");
        assert_eq!(reference_count(&scoping, a_symbol), 1);
        assert_eq!(reference_count(&scoping, b_symbol), 1);

        let mut visitor = RemoveSecondStatement;
        let scoping = traverse_mut(&mut visitor, &allocator, &mut program, scoping, ());

        assert_eq!(reference_count(&scoping, b_symbol), 0, "b ref should be cleaned up");
        assert_eq!(reference_count(&scoping, a_symbol), 1, "a ref should be preserved");
    }

    // -----------------------------------------------------------------------
    // insert_statement / append_statement tests
    // -----------------------------------------------------------------------

    /// Visitor that appends a new statement referencing an existing binding.
    struct AppendReferenceStatement;

    impl<'a> Traverse<'a, ()> for AppendReferenceStatement {
        fn enter_statements(
            &mut self,
            statements: &mut ArenaVec<'a, Statement<'a>>,
            context: &mut TraverseCtx<'a, ()>,
        ) {
            // Only act on the program-level statement list.
            if statements.len() < 2 {
                return;
            }

            // Find the symbol for `x` and create a new reference to it.
            let symbol_id = context
                .scoping()
                .symbol_ids()
                .find(|&id| context.scoping().symbol_name(id) == "x");

            let Some(symbol_id) = symbol_id else {
                return;
            };

            let name: oxc_span::Ident<'a> = context.ast.atom("x").into();
            let expression = context.create_bound_ident_expr(
                SPAN,
                name,
                symbol_id,
                ReferenceFlags::Read,
            );
            let new_statement = context.ast.statement_expression(SPAN, expression);
            append_statement(statements, new_statement);
        }
    }

    #[test]
    fn append_statement_with_new_reference_is_tracked() {
        let allocator = Allocator::default();
        let (mut program, scoping) = parse_and_analyze(&allocator, "let x = 1; x;");

        let x_symbol = find_symbol(&scoping, "x").expect("should find symbol x");
        assert_eq!(reference_count(&scoping, x_symbol), 1);

        let mut visitor = AppendReferenceStatement;
        let scoping = traverse_mut(&mut visitor, &allocator, &mut program, scoping, ());

        // The appended statement adds a new reference to x.
        assert_eq!(
            reference_count(&scoping, x_symbol),
            2,
            "x should have 2 references after append"
        );
    }

    // -----------------------------------------------------------------------
    // replace_expression with new bound reference
    // -----------------------------------------------------------------------

    /// Visitor that replaces a numeric literal `42` with a reference to binding `x`.
    struct ReplaceLiteralWithReference;

    impl<'a> Traverse<'a, ()> for ReplaceLiteralWithReference {
        fn enter_expression(
            &mut self,
            expression: &mut Expression<'a>,
            context: &mut TraverseCtx<'a, ()>,
        ) {
            let Expression::NumericLiteral(literal) = expression else {
                return;
            };
            if literal.value != 42.0 {
                return;
            }

            let symbol_id = context
                .scoping()
                .symbol_ids()
                .find(|&id| context.scoping().symbol_name(id) == "x");

            let Some(symbol_id) = symbol_id else {
                return;
            };

            let name: oxc_span::Ident<'a> = context.ast.atom("x").into();
            let replacement = context.create_bound_ident_expr(
                SPAN,
                name,
                symbol_id,
                ReferenceFlags::Read,
            );
            replace_expression(expression, replacement, context);
        }
    }

    #[test]
    fn replace_expression_with_new_reference_preserves_it() {
        let allocator = Allocator::default();
        let (mut program, scoping) = parse_and_analyze(&allocator, "let x = 1; 42;");

        let x_symbol = find_symbol(&scoping, "x").expect("should find symbol x");
        assert_eq!(reference_count(&scoping, x_symbol), 0, "x should start with 0 references");

        let mut visitor = ReplaceLiteralWithReference;
        let scoping = traverse_mut(&mut visitor, &allocator, &mut program, scoping, ());

        assert_eq!(
            reference_count(&scoping, x_symbol),
            1,
            "x should have 1 reference after replacement"
        );
    }

    // -----------------------------------------------------------------------
    // Multiple references in complex expression
    // -----------------------------------------------------------------------

    #[test]
    fn replace_expression_with_multiple_refs_cleans_up_all() {
        // `a + b + c` replaced with a literal should clean up all 3 references.
        let allocator = Allocator::default();
        let (mut program, scoping) =
            parse_and_analyze(&allocator, "let a = 1; let b = 2; let c = 3; a + b + c;");

        let a_symbol = find_symbol(&scoping, "a").expect("a");
        let b_symbol = find_symbol(&scoping, "b").expect("b");
        let c_symbol = find_symbol(&scoping, "c").expect("c");
        assert_eq!(reference_count(&scoping, a_symbol), 1);
        assert_eq!(reference_count(&scoping, b_symbol), 1);
        assert_eq!(reference_count(&scoping, c_symbol), 1);

        // Use a visitor that replaces any binary expression with `0`.
        struct ReplaceBinaryWithZero;
        impl<'a> Traverse<'a, ()> for ReplaceBinaryWithZero {
            fn enter_expression(
                &mut self,
                expression: &mut Expression<'a>,
                context: &mut TraverseCtx<'a, ()>,
            ) {
                if matches!(expression, Expression::BinaryExpression(_)) {
                    let raw = context.ast.atom("0");
                    let replacement = context.ast.expression_numeric_literal(
                        SPAN,
                        0.0,
                        Some(raw),
                        NumberBase::Decimal,
                    );
                    replace_expression(expression, replacement, context);
                }
            }
        }

        let mut visitor = ReplaceBinaryWithZero;
        let scoping = traverse_mut(&mut visitor, &allocator, &mut program, scoping, ());

        assert_eq!(reference_count(&scoping, a_symbol), 0, "a refs should be cleaned up");
        assert_eq!(reference_count(&scoping, b_symbol), 0, "b refs should be cleaned up");
        assert_eq!(reference_count(&scoping, c_symbol), 0, "c refs should be cleaned up");
    }
}
