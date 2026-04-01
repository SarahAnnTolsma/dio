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
use oxc_syntax::symbol::SymbolId;
use oxc_traverse::TraverseCtx;

use crate::operations;
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

        // Phase 1: Detect dispatch functions.
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

        // Phase 2: Resolve arithmetic state transitions to direct assignments.
        // Convert `df += 625` (in case 72) to `df = 697` for readability.
        let mut changed = false;

        for statement in statements.iter_mut() {
            // Find dispatch functions (var M6 = function lK(...) or function lK(...))
            let function = match statement {
                Statement::VariableDeclaration(declaration) => {
                    declaration.declarations.iter_mut().find_map(|d| {
                        if let Some(Expression::FunctionExpression(f)) = &mut d.init
                            && let Some(binding) = &f.id
                            && let Some(sym) = binding.symbol_id.get()
                            && dispatchers.contains_key(&sym)
                        {
                            return Some(&mut **f);
                        }
                        None
                    })
                }
                Statement::FunctionDeclaration(f) => {
                    if let Some(binding) = &f.id {
                        if let Some(sym) = binding.symbol_id.get() {
                            if dispatchers.contains_key(&sym) {
                                Some(&mut **f)
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
                _ => None,
            };

            let Some(function) = function else {
                continue;
            };
            let Some(body) = &mut function.body else {
                continue;
            };

            // Find the while loop and its switch.
            let Some(while_stmt) = body.statements.iter_mut().find_map(|s| {
                if let Statement::WhileStatement(w) = s {
                    Some(w)
                } else {
                    None
                }
            }) else {
                continue;
            };
            let Statement::BlockStatement(while_body) = &mut while_stmt.body else {
                continue;
            };
            let Some(switch) = while_body.body.iter_mut().find_map(|s| {
                if let Statement::SwitchStatement(sw) = s {
                    Some(sw)
                } else {
                    None
                }
            }) else {
                continue;
            };

            let state_param = &dispatchers
                .values()
                .next()
                .unwrap()
                .state_param_name
                .clone();

            // Resolve arithmetic transitions in each case.
            for case in switch.cases.iter_mut() {
                let Some(test) = &case.test else {
                    continue;
                };
                let Some(case_value) = extract_numeric_value(test) else {
                    continue;
                };

                for stmt in case.consequent.iter_mut() {
                    let Statement::ExpressionStatement(expr_stmt) = stmt else {
                        continue;
                    };
                    let Expression::AssignmentExpression(assignment) = &mut expr_stmt.expression
                    else {
                        continue;
                    };
                    let oxc_ast::ast::AssignmentTarget::AssignmentTargetIdentifier(target) =
                        &assignment.left
                    else {
                        continue;
                    };
                    if target.name.as_str() != state_param {
                        continue;
                    }

                    // Resolve += and -= to direct assignment.
                    match assignment.operator {
                        oxc_syntax::operator::AssignmentOperator::Addition => {
                            if let Some(delta) = extract_numeric_value(&assignment.right) {
                                let resolved = case_value + delta;
                                assignment.operator =
                                    oxc_syntax::operator::AssignmentOperator::Assign;
                                let raw = context.ast.atom(&resolved.to_string());
                                assignment.right = context.ast.expression_numeric_literal(
                                    SPAN,
                                    resolved as f64,
                                    Some(raw),
                                    oxc_syntax::number::NumberBase::Decimal,
                                );
                                changed = true;
                            }
                        }
                        oxc_syntax::operator::AssignmentOperator::Subtraction => {
                            if let Some(delta) = extract_numeric_value(&assignment.right) {
                                let resolved = case_value - delta;
                                assignment.operator =
                                    oxc_syntax::operator::AssignmentOperator::Assign;
                                let raw = context.ast.atom(&resolved.to_string());
                                assignment.right = context.ast.expression_numeric_literal(
                                    SPAN,
                                    resolved as f64,
                                    Some(raw),
                                    oxc_syntax::number::NumberBase::Decimal,
                                );
                                changed = true;
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        // Phase 3: Re-analyze the dispatch function with resolved transitions,
        // then find and replace call sites with linearized bodies.
        // We need to re-analyze because Phase 2 modified the transitions.
        let mut updated_dispatchers: HashMap<SymbolId, DispatchFunction> = HashMap::new();
        for statement in statements.iter() {
            if let Statement::VariableDeclaration(declaration) = statement {
                for declarator in &declaration.declarations {
                    if let Some(Expression::FunctionExpression(function)) = &declarator.init
                        && let Some(dispatch) = analyze_dispatch_function(function)
                    {
                        if let Some(binding) = &function.id
                            && let Some(symbol_id) = binding.symbol_id.get()
                            && dispatchers.contains_key(&symbol_id)
                        {
                            updated_dispatchers.insert(symbol_id, dispatch.clone());
                        }
                        if let oxc_ast::ast::BindingPattern::BindingIdentifier(var_binding) =
                            &declarator.id
                            && let Some(var_symbol) = var_binding.symbol_id.get()
                            && dispatchers.contains_key(&var_symbol)
                        {
                            updated_dispatchers.insert(var_symbol, dispatch);
                        }
                    }
                }
            }
            if let Statement::FunctionDeclaration(function) = statement
                && let Some(dispatch) = analyze_dispatch_function(function)
                && let Some(binding) = &function.id
                && let Some(symbol_id) = binding.symbol_id.get()
                && dispatchers.contains_key(&symbol_id)
            {
                updated_dispatchers.insert(symbol_id, dispatch);
            }
        }

        // Find call sites: expression statements that call a dispatch function
        // with a known numeric entry value.
        let mut call_replacements: Vec<(usize, SymbolId, i64)> = Vec::new();
        for (index, statement) in statements.iter().enumerate() {
            let Statement::ExpressionStatement(expr_stmt) = statement else {
                continue;
            };
            if let Some((symbol_id, entry_value)) =
                extract_dispatch_call(&expr_stmt.expression, &updated_dispatchers, context)
            {
                call_replacements.push((index, symbol_id, entry_value));
            }
        }

        // Build a plan: for each call site, compute which (case_index, stmt_indices)
        // to extract, in order. This is done immutably.
        #[allow(clippy::type_complexity)]
        let mut linearization_plan: Vec<(usize, Vec<(usize, Vec<usize>)>)> = Vec::new();

        for &(call_index, symbol_id, entry_value) in call_replacements.iter().rev() {
            let Some(dispatch) = updated_dispatchers.get(&symbol_id) else {
                continue;
            };
            let Some(traced_states) = trace_dispatch(entry_value, dispatch) else {
                continue;
            };

            let mut case_extractions: Vec<(usize, Vec<usize>)> = Vec::new();
            for &state in &traced_states {
                if let Some(case_info) = dispatch.cases.get(&state)
                    && !case_info.statement_indices.is_empty()
                {
                    case_extractions
                        .push((case_info.case_index, case_info.statement_indices.clone()));
                }
            }

            if !case_extractions.is_empty() {
                linearization_plan.push((call_index, case_extractions));
            }
        }

        // Execute the plan: extract statements from the switch and replace call sites.
        // Find the dispatch function statement index once.
        let func_stmt_index = find_dispatch_switch_index(statements, &dispatchers);

        for (call_index, case_extractions) in &linearization_plan {
            let Some(func_index) = func_stmt_index else {
                continue;
            };

            // Navigate to the switch inside the function.
            let switch = navigate_to_switch(&mut statements[func_index]);
            let Some(switch) = switch else {
                continue;
            };

            let mut replacement_stmts: Vec<Statement<'a>> = Vec::new();
            for (case_idx, stmt_indices) in case_extractions {
                let case = &mut switch.cases[*case_idx];
                for &stmt_idx in stmt_indices {
                    if stmt_idx < case.consequent.len() {
                        let stmt = std::mem::replace(
                            &mut case.consequent[stmt_idx],
                            context.ast.statement_empty(SPAN),
                        );
                        replacement_stmts.push(stmt);
                    }
                }
            }

            if !replacement_stmts.is_empty() {
                let arena_replacements = context.ast.vec_from_iter(replacement_stmts);
                operations::replace_statement_with_multiple(
                    statements,
                    *call_index,
                    arena_replacements,
                    context,
                );
                changed = true;
            }
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
