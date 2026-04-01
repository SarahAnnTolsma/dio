//! Linearizes Akamai's switch-based dispatch functions.
//!
//! Akamai uses functions like:
//! ```js
//! var M6 = function lK(df, Mh) {
//!     while (df != 319) {
//!         switch (df) {
//!             case 295: code; df = 72; break;
//!             case 72: code; df += 625; break;
//!             case 697: return result;
//!         }
//!     }
//! };
//! ```
//!
//! For each known call site like `lK(295, args)`, we trace the state
//! transitions and generate a linearized function `lK_295(Mh)`.

use std::collections::HashMap;
use std::sync::Mutex;

use oxc_allocator::Vec as ArenaVec;
use oxc_ast::ast::{Expression, Statement};
use oxc_span::SPAN;
use oxc_syntax::scope::ScopeFlags;
use oxc_syntax::symbol::SymbolId;
use oxc_traverse::TraverseCtx;

use crate::transformer::{AstNodeType, Transformer, TransformerPhase, TransformerPriority};
use crate::utils::unwrap_parens;

/// Information about a dispatch function's switch structure.
#[derive(Debug, Clone)]
struct DispatchFunction {
    /// The state parameter name (e.g., "df").
    state_param_name: String,
    /// The exit value (e.g., 319 from `while (df != 319)`).
    exit_value: i64,
    /// Map from case value to case body info.
    cases: HashMap<i64, CaseInfo>,
    /// Entry values discovered from call sites (populated across passes).
    discovered_entry_values: Vec<i64>,
    /// Entry values already linearized (functions generated).
    linearized_entry_values: Vec<i64>,
}

/// Information about a single switch case.
#[derive(Debug, Clone)]
struct CaseInfo {
    /// Index of this case in the switch's cases array.
    case_index: usize,
    /// Indices of statements to extract (excluding state assignment + break).
    statement_indices: Vec<usize>,
    /// The transition: None = exit, Some(value) = next state.
    transition: CaseTransition,
}

/// How a case transitions to the next state.
#[derive(Debug, Clone)]
enum CaseTransition {
    /// `df = N; break;` or `df += N; break;` — resolved to a concrete value.
    Direct(i64),
    /// `return expr;` — terminal, no next state.
    Return,
    /// `df = 319; break;` — exit the while loop.
    Exit,
}

/// Tracked dispatch functions and their resolved entry points.
pub struct SwitchDispatchTransformer {
    /// Maps function symbol ID → dispatch info.
    dispatchers: Mutex<HashMap<SymbolId, DispatchFunction>>,
}

impl Default for SwitchDispatchTransformer {
    fn default() -> Self {
        Self {
            dispatchers: Mutex::new(HashMap::new()),
        }
    }
}

impl Transformer for SwitchDispatchTransformer {
    fn name(&self) -> &str {
        "SwitchDispatchTransformer"
    }

    fn interests(&self) -> &[AstNodeType] {
        &[AstNodeType::StatementList]
    }

    fn priority(&self) -> TransformerPriority {
        TransformerPriority::Default
    }

    fn phase(&self) -> TransformerPhase {
        TransformerPhase::Main
    }

    fn enter_statements<'a>(
        &self,
        statements: &mut ArenaVec<'a, Statement<'a>>,
        context: &mut TraverseCtx<'a, ()>,
    ) -> bool {
        let mut dispatchers = self.dispatchers.lock().unwrap();

        // Phase 1: Detect dispatch functions (only on first pass).
        if dispatchers.is_empty() {
            for statement in statements.iter() {
                // Check variable declarations: var M6 = function lK(df, Mh) { ... }
                if let Statement::VariableDeclaration(declaration) = statement {
                    for declarator in &declaration.declarations {
                        if let Some(init) = &declarator.init
                            && let Expression::FunctionExpression(function) = init
                            && let Some(dispatch) = analyze_dispatch_function(function)
                        {
                            // Register both the function expression name (lK) and
                            // the variable name (M6) so calls via either name match.
                            if let Some(binding) = &function.id
                                && let Some(symbol_id) = binding.symbol_id.get()
                            {
                                dispatchers.insert(symbol_id, dispatch.clone());
                            }
                            if let oxc_ast::ast::BindingPattern::BindingIdentifier(var_binding) =
                                &declarator.id
                                && let Some(var_symbol) = var_binding.symbol_id.get()
                            {
                                dispatchers.insert(var_symbol, dispatch);
                            }
                        }
                    }
                }
                // Check function declarations: function lK(df, Mh) { ... }
                if let Statement::FunctionDeclaration(function) = statement
                    && let Some(dispatch) = analyze_dispatch_function(function)
                    && let Some(binding) = &function.id
                    && let Some(symbol_id) = binding.symbol_id.get()
                {
                    dispatchers.insert(symbol_id, dispatch);
                }
            }
        }

        if dispatchers.is_empty() {
            return false;
        }

        // Scan this scope for dispatch calls and record entry values.
        for statement in statements.iter() {
            scan_for_dispatch_calls(statement, &mut dispatchers, context);
        }

        let mut changed = false;

        // Phase 2: Resolve arithmetic state transitions (only in the scope
        // that contains the dispatch function).
        let func_stmt_index = find_dispatch_switch_index(statements, &dispatchers);

        if let Some(func_index) = func_stmt_index {
            // Phase 2: Resolve arithmetic transitions in the switch.
            let state_param = dispatchers
                .values()
                .next()
                .map(|d| d.state_param_name.clone());

            if let Some(ref state_param) = state_param
                && let Some(switch) = navigate_to_switch(&mut statements[func_index])
            {
                for case in switch.cases.iter_mut() {
                    let case_value = case.test.as_ref().and_then(extract_numeric_value);
                    let Some(case_value) = case_value else {
                        continue;
                    };
                    for stmt in case.consequent.iter_mut() {
                        changed |=
                            resolve_arithmetic_transition(stmt, state_param, case_value, context);
                    }
                }
            }

            // Re-analyze with resolved transitions.
            let updated_dispatch = match &statements[func_index] {
                Statement::VariableDeclaration(declaration) => {
                    declaration.declarations.iter().find_map(|d| {
                        if let Some(Expression::FunctionExpression(f)) = &d.init {
                            analyze_dispatch_function(f)
                        } else {
                            None
                        }
                    })
                }
                Statement::FunctionDeclaration(f) => analyze_dispatch_function(f),
                _ => None,
            };

            // Phase 3a: Generate linearized functions for discovered entry values.
            if let Some(dispatch) = &updated_dispatch {
                let entry_values: Vec<i64> = dispatchers
                    .values()
                    .flat_map(|d| d.discovered_entry_values.iter().copied())
                    .collect();
                let already_linearized: Vec<i64> = dispatchers
                    .values()
                    .flat_map(|d| d.linearized_entry_values.iter().copied())
                    .collect();
                let func_name = get_dispatch_func_name(&statements[func_index]);

                #[allow(clippy::type_complexity)]
                let mut new_functions: Vec<(i64, Vec<Statement<'a>>)> = Vec::new();
                for entry_value in &entry_values {
                    if already_linearized.contains(entry_value) {
                        continue;
                    }
                    let Some(traced) = trace_dispatch(*entry_value, dispatch) else {
                        continue;
                    };

                    if let Some(switch) = navigate_to_switch(&mut statements[func_index]) {
                        let mut body_stmts: Vec<Statement<'a>> = Vec::new();
                        for &state in &traced {
                            if let Some(case_info) = dispatch.cases.get(&state) {
                                let case = &mut switch.cases[case_info.case_index];
                                for &stmt_idx in &case_info.statement_indices {
                                    if stmt_idx < case.consequent.len() {
                                        let stmt = std::mem::replace(
                                            &mut case.consequent[stmt_idx],
                                            context.ast.statement_empty(SPAN),
                                        );
                                        body_stmts.push(stmt);
                                    }
                                }
                            }
                        }
                        if !body_stmts.is_empty() {
                            new_functions.push((*entry_value, body_stmts));
                        }
                    }
                }

                // Insert new functions after the dispatch function.
                let mut insert_offset = 1;
                for (entry_value, body_stmts) in new_functions {
                    let new_name = format!(
                        "{}_{}",
                        func_name.as_deref().unwrap_or("dispatch"),
                        entry_value
                    );
                    let name_atom = context.ast.atom(&new_name);
                    let binding = context.ast.binding_identifier(SPAN, name_atom);

                    let param_atom = context.ast.atom("Mh");
                    let param_pattern = context
                        .ast
                        .binding_pattern_binding_identifier(SPAN, param_atom);
                    let param = context.ast.formal_parameter(
                        SPAN,
                        context.ast.vec(),
                        param_pattern,
                        oxc_ast::NONE,
                        oxc_ast::NONE,
                        false,
                        None,
                        false,
                        false,
                    );
                    let params = context.ast.formal_parameters(
                        SPAN,
                        oxc_ast::ast::FormalParameterKind::FormalParameter,
                        context.ast.vec_from_iter([param]),
                        oxc_ast::NONE,
                    );

                    let body_vec = context.ast.vec_from_iter(body_stmts);
                    let function_body =
                        context
                            .ast
                            .alloc_function_body(SPAN, context.ast.vec(), body_vec);
                    let func_node = context.ast.alloc_function(
                        SPAN,
                        oxc_ast::ast::FunctionType::FunctionDeclaration,
                        Some(binding),
                        false,
                        false,
                        false,
                        oxc_ast::NONE,
                        oxc_ast::NONE,
                        params,
                        oxc_ast::NONE,
                        Some(function_body),
                    );

                    // Register a scope for the new function so oxc's
                    // traversal doesn't panic on the missing scope_id.
                    let scope_id = context.create_child_scope_of_current(
                        ScopeFlags::Function | ScopeFlags::StrictMode,
                    );
                    func_node.scope_id.set(Some(scope_id));

                    let function = Statement::FunctionDeclaration(func_node);
                    statements.insert(func_index + insert_offset, function);
                    insert_offset += 1;
                    changed = true;

                    for dispatch in dispatchers.values_mut() {
                        if !dispatch.linearized_entry_values.contains(&entry_value) {
                            dispatch.linearized_entry_values.push(entry_value);
                        }
                    }
                }
            }
        }

        // Phase 3b: Rewrite dispatch calls in this scope.
        // M6(15, args) -> M6_15(args)
        for statement in statements.iter_mut() {
            changed |= rewrite_dispatch_calls(statement, &dispatchers, context);
        }

        changed
    }
}

/// Find the statement index containing the dispatch function.
fn find_dispatch_switch_index(
    statements: &[Statement<'_>],
    dispatchers: &HashMap<SymbolId, DispatchFunction>,
) -> Option<usize> {
    for (index, statement) in statements.iter().enumerate() {
        match statement {
            Statement::VariableDeclaration(declaration) => {
                for declarator in &declaration.declarations {
                    if let Some(Expression::FunctionExpression(function)) = &declarator.init {
                        // Check function expression name.
                        if let Some(binding) = &function.id
                            && let Some(symbol_id) = binding.symbol_id.get()
                            && dispatchers.contains_key(&symbol_id)
                        {
                            return Some(index);
                        }
                        // Check variable name.
                        if let oxc_ast::ast::BindingPattern::BindingIdentifier(var_binding) =
                            &declarator.id
                            && let Some(var_symbol) = var_binding.symbol_id.get()
                            && dispatchers.contains_key(&var_symbol)
                        {
                            return Some(index);
                        }
                    }
                }
            }
            Statement::FunctionDeclaration(function) => {
                if let Some(binding) = &function.id
                    && let Some(symbol_id) = binding.symbol_id.get()
                    && dispatchers.contains_key(&symbol_id)
                {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

/// Navigate from a statement to the switch inside a dispatch function.
fn navigate_to_switch<'a, 'b>(
    statement: &'b mut Statement<'a>,
) -> Option<&'b mut oxc_ast::ast::SwitchStatement<'a>> {
    let function = match statement {
        Statement::VariableDeclaration(declaration) => {
            declaration.declarations.iter_mut().find_map(|d| {
                if let Some(Expression::FunctionExpression(f)) = &mut d.init {
                    Some(&mut **f)
                } else {
                    None
                }
            })
        }
        Statement::FunctionDeclaration(f) => Some(&mut **f),
        _ => None,
    }?;

    let body = function.body.as_mut()?;
    for stmt in body.statements.iter_mut() {
        if let Statement::WhileStatement(while_stmt) = stmt
            && let Statement::BlockStatement(while_body) = &mut while_stmt.body
        {
            for inner_stmt in while_body.body.iter_mut() {
                if let Statement::SwitchStatement(sw) = inner_stmt {
                    return Some(sw);
                }
            }
        }
    }
    None
}

/// Resolve an arithmetic state transition in place.
///
/// If the statement is `state = state + N` or `state -= N`, resolves it to
/// `state = computed_value` using the known case value.
fn resolve_arithmetic_transition<'a>(
    statement: &mut Statement<'a>,
    state_param: &str,
    case_value: i64,
    context: &mut TraverseCtx<'a, ()>,
) -> bool {
    let Statement::ExpressionStatement(expr_stmt) = statement else {
        return false;
    };
    let Expression::AssignmentExpression(assignment) = &mut expr_stmt.expression else {
        return false;
    };
    let oxc_ast::ast::AssignmentTarget::AssignmentTargetIdentifier(target) = &assignment.left
    else {
        return false;
    };
    if target.name.as_str() != state_param {
        return false;
    }

    let resolved = match assignment.operator {
        oxc_syntax::operator::AssignmentOperator::Addition => {
            extract_numeric_value(&assignment.right).map(|delta| case_value + delta)
        }
        oxc_syntax::operator::AssignmentOperator::Subtraction => {
            extract_numeric_value(&assignment.right).map(|delta| case_value - delta)
        }
        _ => None,
    };

    if let Some(resolved) = resolved {
        assignment.operator = oxc_syntax::operator::AssignmentOperator::Assign;
        let raw = context.ast.atom(&resolved.to_string());
        assignment.right = context.ast.expression_numeric_literal(
            SPAN,
            resolved as f64,
            Some(raw),
            oxc_syntax::number::NumberBase::Decimal,
        );
        true
    } else {
        false
    }
}

/// Walk a statement recursively looking for dispatch call expressions.
///
/// For each call like `M6(15, ...)`, adds 15 to the dispatcher's
/// `discovered_entry_values`. Also handles `.call(thisArg, N, ...)`.
fn scan_for_dispatch_calls(
    statement: &Statement<'_>,
    dispatchers: &mut HashMap<SymbolId, DispatchFunction>,
    context: &TraverseCtx<'_, ()>,
) {
    match statement {
        Statement::ExpressionStatement(expr_stmt) => {
            scan_expression_for_dispatch_calls(&expr_stmt.expression, dispatchers, context);
        }
        Statement::VariableDeclaration(declaration) => {
            for declarator in &declaration.declarations {
                if let Some(init) = &declarator.init {
                    scan_expression_for_dispatch_calls(init, dispatchers, context);
                }
            }
        }
        Statement::ReturnStatement(return_stmt) => {
            if let Some(argument) = &return_stmt.argument {
                scan_expression_for_dispatch_calls(argument, dispatchers, context);
            }
        }
        Statement::IfStatement(if_stmt) => {
            scan_expression_for_dispatch_calls(&if_stmt.test, dispatchers, context);
            scan_for_dispatch_calls(&if_stmt.consequent, dispatchers, context);
            if let Some(alternate) = &if_stmt.alternate {
                scan_for_dispatch_calls(alternate, dispatchers, context);
            }
        }
        Statement::BlockStatement(block) => {
            for stmt in &block.body {
                scan_for_dispatch_calls(stmt, dispatchers, context);
            }
        }
        _ => {}
    }
}

/// Scan an expression recursively for dispatch calls.
fn scan_expression_for_dispatch_calls(
    expression: &Expression<'_>,
    dispatchers: &mut HashMap<SymbolId, DispatchFunction>,
    context: &TraverseCtx<'_, ()>,
) {
    if let Some((symbol_id, entry_value)) = extract_dispatch_call(expression, dispatchers, context)
        && let Some(dispatch) = dispatchers.get_mut(&symbol_id)
        && !dispatch.discovered_entry_values.contains(&entry_value)
    {
        dispatch.discovered_entry_values.push(entry_value);
    }

    // Recurse into sub-expressions.
    match expression {
        Expression::CallExpression(call) => {
            scan_expression_for_dispatch_calls(&call.callee, dispatchers, context);
            for arg in &call.arguments {
                if let Some(expr) = arg.as_expression() {
                    scan_expression_for_dispatch_calls(expr, dispatchers, context);
                }
            }
        }
        Expression::AssignmentExpression(assignment) => {
            scan_expression_for_dispatch_calls(&assignment.right, dispatchers, context);
        }
        Expression::BinaryExpression(binary) => {
            scan_expression_for_dispatch_calls(&binary.left, dispatchers, context);
            scan_expression_for_dispatch_calls(&binary.right, dispatchers, context);
        }
        Expression::ConditionalExpression(cond) => {
            scan_expression_for_dispatch_calls(&cond.test, dispatchers, context);
            scan_expression_for_dispatch_calls(&cond.consequent, dispatchers, context);
            scan_expression_for_dispatch_calls(&cond.alternate, dispatchers, context);
        }
        Expression::SequenceExpression(sequence) => {
            for expr in &sequence.expressions {
                scan_expression_for_dispatch_calls(expr, dispatchers, context);
            }
        }
        Expression::LogicalExpression(logical) => {
            scan_expression_for_dispatch_calls(&logical.left, dispatchers, context);
            scan_expression_for_dispatch_calls(&logical.right, dispatchers, context);
        }
        Expression::UnaryExpression(unary) => {
            scan_expression_for_dispatch_calls(&unary.argument, dispatchers, context);
        }
        Expression::ParenthesizedExpression(paren) => {
            scan_expression_for_dispatch_calls(&paren.expression, dispatchers, context);
        }
        _ => {}
    }
}

/// Extract the function name from a statement containing a dispatch function.
///
/// For `var M6 = function lK(...) { ... }`, returns `"M6"`.
/// For `function lK(...) { ... }`, returns `"lK"`.
fn get_dispatch_func_name(statement: &Statement<'_>) -> Option<String> {
    match statement {
        Statement::VariableDeclaration(declaration) => {
            for declarator in &declaration.declarations {
                if let oxc_ast::ast::BindingPattern::BindingIdentifier(binding) = &declarator.id {
                    return Some(binding.name.to_string());
                }
            }
            None
        }
        Statement::FunctionDeclaration(function) => {
            function.id.as_ref().map(|id| id.name.to_string())
        }
        _ => None,
    }
}

/// Rewrite dispatch calls in a statement.
///
/// Replaces `M6(15, args)` with `M6_15(args)` — changes the callee identifier
/// name and removes the first argument.
fn rewrite_dispatch_calls<'a>(
    statement: &mut Statement<'a>,
    dispatchers: &HashMap<SymbolId, DispatchFunction>,
    context: &mut TraverseCtx<'a, ()>,
) -> bool {
    match statement {
        Statement::ExpressionStatement(expr_stmt) => {
            rewrite_expression_dispatch_calls(&mut expr_stmt.expression, dispatchers, context)
        }
        Statement::VariableDeclaration(declaration) => {
            let mut changed = false;
            for declarator in declaration.declarations.iter_mut() {
                if let Some(init) = &mut declarator.init {
                    changed |= rewrite_expression_dispatch_calls(init, dispatchers, context);
                }
            }
            changed
        }
        Statement::ReturnStatement(return_stmt) => {
            if let Some(argument) = &mut return_stmt.argument {
                rewrite_expression_dispatch_calls(argument, dispatchers, context)
            } else {
                false
            }
        }
        Statement::IfStatement(if_stmt) => {
            let mut changed =
                rewrite_expression_dispatch_calls(&mut if_stmt.test, dispatchers, context);
            changed |= rewrite_dispatch_calls(&mut if_stmt.consequent, dispatchers, context);
            if let Some(alternate) = &mut if_stmt.alternate {
                changed |= rewrite_dispatch_calls(alternate, dispatchers, context);
            }
            changed
        }
        Statement::BlockStatement(block) => {
            let mut changed = false;
            for stmt in block.body.iter_mut() {
                changed |= rewrite_dispatch_calls(stmt, dispatchers, context);
            }
            changed
        }
        _ => false,
    }
}

/// Rewrite dispatch calls within an expression.
fn rewrite_expression_dispatch_calls<'a>(
    expression: &mut Expression<'a>,
    dispatchers: &HashMap<SymbolId, DispatchFunction>,
    context: &mut TraverseCtx<'a, ()>,
) -> bool {
    let mut changed = false;

    match expression {
        Expression::CallExpression(call) => {
            // Check if this is a direct dispatch call: M6(15, ...)
            if let Expression::Identifier(callee) = &call.callee
                && let Some(ref_id) = callee.reference_id.get()
            {
                let reference = context.scoping().get_reference(ref_id);
                if let Some(symbol_id) = reference.symbol_id()
                    && let Some(dispatch) = dispatchers.get(&symbol_id)
                    && !call.arguments.is_empty()
                    && let Some(entry) = call.arguments[0]
                        .as_expression()
                        .and_then(|e| extract_numeric_value(unwrap_parens(e)))
                    && dispatch.linearized_entry_values.contains(&entry)
                {
                    // Get the function name from the callee.
                    let func_name = callee.name.to_string();
                    let new_name = format!("{}_{}", func_name, entry);
                    let new_atom = context.ast.atom(&new_name);

                    // Replace callee with new identifier.
                    call.callee = context.ast.expression_identifier(SPAN, new_atom);

                    // Remove the first argument (the entry value).
                    call.arguments.remove(0);
                    changed = true;
                }
            }

            // Recurse into arguments.
            for arg in call.arguments.iter_mut() {
                if let Some(expr) = arg.as_expression_mut() {
                    changed |= rewrite_expression_dispatch_calls(expr, dispatchers, context);
                }
            }
            // Recurse into callee (for chained calls).
            changed |= rewrite_expression_dispatch_calls(&mut call.callee, dispatchers, context);
        }
        Expression::AssignmentExpression(assignment) => {
            changed |=
                rewrite_expression_dispatch_calls(&mut assignment.right, dispatchers, context);
        }
        Expression::BinaryExpression(binary) => {
            changed |= rewrite_expression_dispatch_calls(&mut binary.left, dispatchers, context);
            changed |= rewrite_expression_dispatch_calls(&mut binary.right, dispatchers, context);
        }
        Expression::ConditionalExpression(cond) => {
            changed |= rewrite_expression_dispatch_calls(&mut cond.test, dispatchers, context);
            changed |=
                rewrite_expression_dispatch_calls(&mut cond.consequent, dispatchers, context);
            changed |= rewrite_expression_dispatch_calls(&mut cond.alternate, dispatchers, context);
        }
        Expression::SequenceExpression(sequence) => {
            for expr in sequence.expressions.iter_mut() {
                changed |= rewrite_expression_dispatch_calls(expr, dispatchers, context);
            }
        }
        Expression::LogicalExpression(logical) => {
            changed |= rewrite_expression_dispatch_calls(&mut logical.left, dispatchers, context);
            changed |= rewrite_expression_dispatch_calls(&mut logical.right, dispatchers, context);
        }
        Expression::UnaryExpression(unary) => {
            changed |= rewrite_expression_dispatch_calls(&mut unary.argument, dispatchers, context);
        }
        Expression::ParenthesizedExpression(paren) => {
            changed |=
                rewrite_expression_dispatch_calls(&mut paren.expression, dispatchers, context);
        }
        _ => {}
    }

    changed
}

/// Analyze a function to determine if it's a dispatch function.
///
/// Pattern:
/// ```js
/// function lK(df, Mh) {
///     while (df != EXIT_VALUE) {
///         switch (df) { ... }
///     }
/// }
/// ```
fn analyze_dispatch_function(function: &oxc_ast::ast::Function<'_>) -> Option<DispatchFunction> {
    // Must have at least one parameter (the state variable).
    if function.params.items.is_empty() {
        return None;
    }

    let state_param_name = if let oxc_ast::ast::BindingPattern::BindingIdentifier(binding) =
        &function.params.items[0].pattern
    {
        binding.name.to_string()
    } else {
        return None;
    };

    let body = function.body.as_ref()?;

    // Find the while loop: while (df != EXIT) { switch (df) { ... } }
    // May have a var declaration before the while (e.g., `var LK = lK;`)
    let while_statement = body.statements.iter().find_map(|stmt| {
        if let Statement::WhileStatement(while_stmt) = stmt {
            Some(while_stmt)
        } else {
            None
        }
    })?;

    // Extract exit value from: while (df != 319)
    let exit_value = extract_while_exit_value(while_statement, &state_param_name)?;

    // The while body must be a block containing a switch statement.
    let Statement::BlockStatement(while_body) = &while_statement.body else {
        return None;
    };

    let switch = while_body.body.iter().find_map(|stmt| {
        if let Statement::SwitchStatement(switch) = stmt {
            Some(switch)
        } else {
            None
        }
    })?;

    // Switch discriminant must be the state parameter.
    let Expression::Identifier(discriminant) = &switch.discriminant else {
        return None;
    };
    if discriminant.name.as_str() != state_param_name {
        return None;
    }

    // Parse all cases.
    let mut cases = HashMap::new();
    for (case_index, case) in switch.cases.iter().enumerate() {
        let Some(test) = &case.test else {
            continue;
        };
        let Some(case_value) = extract_numeric_value(test) else {
            continue;
        };

        let (statement_indices, transition) =
            parse_case_body(&case.consequent, &state_param_name, case_value, exit_value);

        cases.insert(
            case_value,
            CaseInfo {
                case_index,
                statement_indices,
                transition,
            },
        );
    }

    if cases.is_empty() {
        return None;
    }

    Some(DispatchFunction {
        state_param_name,
        exit_value,
        cases,
        discovered_entry_values: Vec::new(),
        linearized_entry_values: Vec::new(),
    })
}

/// Extract the exit value from `while (df != 319)`.
fn extract_while_exit_value(
    while_stmt: &oxc_ast::ast::WhileStatement<'_>,
    state_name: &str,
) -> Option<i64> {
    let Expression::BinaryExpression(binary) = unwrap_parens(&while_stmt.test) else {
        return None;
    };

    if binary.operator != oxc_syntax::operator::BinaryOperator::Inequality
        && binary.operator != oxc_syntax::operator::BinaryOperator::StrictInequality
    {
        return None;
    }

    // Check: df != N or N != df
    if let Expression::Identifier(id) = unwrap_parens(&binary.left)
        && id.name.as_str() == state_name
    {
        return extract_numeric_value(unwrap_parens(&binary.right));
    }
    if let Expression::Identifier(id) = unwrap_parens(&binary.right)
        && id.name.as_str() == state_name
    {
        return extract_numeric_value(unwrap_parens(&binary.left));
    }

    None
}

/// Parse a case body to extract statement indices and the state transition.
fn parse_case_body(
    consequent: &[Statement<'_>],
    state_name: &str,
    case_value: i64,
    exit_value: i64,
) -> (Vec<usize>, CaseTransition) {
    let mut statement_indices = Vec::new();
    let mut transition = CaseTransition::Exit;

    for (idx, stmt) in consequent.iter().enumerate() {
        // break — end of case
        if matches!(stmt, Statement::BreakStatement(_)) {
            break;
        }

        // return — terminal
        if matches!(stmt, Statement::ReturnStatement(_)) {
            statement_indices.push(idx);
            transition = CaseTransition::Return;
            break;
        }

        // Check for state assignment: df = N, df += N, df -= N
        if let Some(next_state) = extract_state_transition(stmt, state_name, case_value) {
            if next_state == exit_value {
                transition = CaseTransition::Exit;
            } else {
                transition = CaseTransition::Direct(next_state);
            }
            continue;
        }

        statement_indices.push(idx);
    }

    (statement_indices, transition)
}

/// Extract a state transition from a statement: `df = N`, `df += N`, `df -= N`.
fn extract_state_transition(
    statement: &Statement<'_>,
    state_name: &str,
    current_value: i64,
) -> Option<i64> {
    let Statement::ExpressionStatement(expression_statement) = statement else {
        return None;
    };
    let Expression::AssignmentExpression(assignment) = &expression_statement.expression else {
        return None;
    };

    let oxc_ast::ast::AssignmentTarget::AssignmentTargetIdentifier(target) = &assignment.left
    else {
        return None;
    };
    if target.name.as_str() != state_name {
        return None;
    }

    match assignment.operator {
        oxc_syntax::operator::AssignmentOperator::Assign => {
            extract_numeric_value(&assignment.right)
        }
        oxc_syntax::operator::AssignmentOperator::Addition => {
            let delta = extract_numeric_value(&assignment.right)?;
            Some(current_value + delta)
        }
        oxc_syntax::operator::AssignmentOperator::Subtraction => {
            let delta = extract_numeric_value(&assignment.right)?;
            Some(current_value - delta)
        }
        _ => None,
    }
}

/// Extract a dispatch call: lK(numericLiteral, ...) or lK.call(thisArg, numericLiteral, ...)
fn extract_dispatch_call(
    expression: &Expression<'_>,
    dispatchers: &HashMap<SymbolId, DispatchFunction>,
    context: &TraverseCtx<'_, ()>,
) -> Option<(SymbolId, i64)> {
    let Expression::CallExpression(call) = expression else {
        return None;
    };

    // Direct call: lK(numeric, ...)
    if let Expression::Identifier(callee) = &call.callee {
        let ref_id = callee.reference_id.get()?;
        let reference = context.scoping().get_reference(ref_id);
        let symbol_id = reference.symbol_id()?;
        if dispatchers.contains_key(&symbol_id)
            && !call.arguments.is_empty()
            && let Some(entry) = call.arguments[0]
                .as_expression()
                .and_then(|e| extract_numeric_value(unwrap_parens(e)))
        {
            return Some((symbol_id, entry));
        }
    }

    None
}

/// Trace through the dispatch function starting from an entry value.
/// Returns the ordered list of states visited.
fn trace_dispatch(entry_value: i64, dispatch: &DispatchFunction) -> Option<Vec<i64>> {
    let mut states = Vec::new();
    let mut current = entry_value;
    let mut visited = Vec::new();

    loop {
        if visited.contains(&current) {
            return None; // Cycle detected — can't linearize.
        }
        visited.push(current);

        let case = dispatch.cases.get(&current)?;
        states.push(current);

        match &case.transition {
            CaseTransition::Direct(next) => {
                current = *next;
            }
            CaseTransition::Return | CaseTransition::Exit => {
                break;
            }
        }
    }

    Some(states)
}

/// Extract a numeric value from an expression.
fn extract_numeric_value(expression: &Expression<'_>) -> Option<i64> {
    match expression {
        Expression::NumericLiteral(number) => Some(number.value as i64),
        Expression::UnaryExpression(unary)
            if unary.operator == oxc_syntax::operator::UnaryOperator::UnaryNegation =>
        {
            if let Expression::NumericLiteral(number) = &unary.argument {
                Some(-(number.value as i64))
            } else {
                None
            }
        }
        _ => None,
    }
}
