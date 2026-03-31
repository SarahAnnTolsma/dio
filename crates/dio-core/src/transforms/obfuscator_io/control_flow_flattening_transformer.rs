//! Linearizes control flow flattening (state machine switches).
//!
//! Obfuscator.io flattens sequential code into a `for/switch` state machine:
//!
//! ```js
//! for (var state = INIT; true;) {
//!     switch (state) {
//!         case X: case Y: break;          // exit
//!         case Z: case W: code; state = NEXT; continue;  // transition
//!         case D: case D: dead_code; continue;  // dead (never reached)
//!     }
//!     break;
//! }
//! ```
//!
//! This transformer traces the state transitions starting from the initial
//! state, collects statements in execution order, and replaces the entire
//! for/switch with the linearized code.

use oxc_allocator::Vec as ArenaVec;
use oxc_ast::ast::{Expression, Statement};
use oxc_span::SPAN;
use oxc_traverse::TraverseCtx;

use crate::operations;
use crate::transformer::{AstNodeType, Transformer, TransformerPhase, TransformerPriority};

/// Linearizes for/switch state machine patterns.
pub struct ControlFlowFlatteningTransformer;

impl Transformer for ControlFlowFlatteningTransformer {
    fn name(&self) -> &str {
        "ControlFlowFlatteningTransformer"
    }

    fn interests(&self) -> &[AstNodeType] {
        &[AstNodeType::StatementList]
    }

    fn priority(&self) -> TransformerPriority {
        // Must run before constant inlining (First priority) replaces
        // the state variable with its initial value.
        TransformerPriority::First
    }

    fn phase(&self) -> TransformerPhase {
        TransformerPhase::Main
    }

    fn enter_statements<'a>(
        &self,
        statements: &mut ArenaVec<'a, Statement<'a>>,
        context: &mut TraverseCtx<'a, ()>,
    ) -> bool {
        let mut changed = false;

        for index in (0..statements.len()).rev() {
            let Statement::ForStatement(for_statement) = &statements[index] else {
                continue;
            };


            // Condition must be `true`.
            let Some(test) = &for_statement.test else {
                continue;
            };
            let Expression::BooleanLiteral(boolean) = test else {
                continue;
            };
            if !boolean.value {
                continue;
            }

            // Body must be a block with exactly: switch(...) { ... } break;
            let Statement::BlockStatement(body) = &for_statement.body else {
                continue;
            };
            if body.body.len() != 2 {
                continue;
            }
            if !matches!(&body.body[1], Statement::BreakStatement(_)) {
                continue;
            }
            let Statement::SwitchStatement(switch) = &body.body[0] else {
                continue;
            };


            // Switch discriminant must be a simple identifier (the state variable).
            let Expression::Identifier(state_identifier) = &switch.discriminant else {
                continue;
            };
            let state_name = state_identifier.name.to_string();

            // The for-init must declare or assign the state variable to a numeric literal.
            let Some(initial_state) =
                extract_initial_state(&for_statement.init, &state_name)
            else {
                continue;
            };


            // Parse all switch cases into a state map.
            let Some(state_map) = parse_state_map(switch, &state_name) else {
                continue;
            };


            // Trace through the state machine starting from initial_state.
            let Some(linearized) = trace_states(initial_state, &state_map) else {
                continue;
            };

            // (linearized may be empty if the state machine had no real code)

            // Build replacement statements.
            let mut replacement_statements: Vec<Statement<'a>> = Vec::new();

            // Re-borrow mutably to extract statements.
            let Statement::ForStatement(for_statement) = &mut statements[index] else {
                continue;
            };

            // Keep the for-init as a variable declaration statement.
            // The state variable will be removed later by UnusedVariableTransformer.
            if let Some(oxc_ast::ast::ForStatementInit::VariableDeclaration(init_decl)) =
                &mut for_statement.init
            {
                // Remove the initializer from the state variable so it becomes
                // `var state;` instead of `var state = N;`.
                for declarator in init_decl.declarations.iter_mut() {
                    if let oxc_ast::ast::BindingPattern::BindingIdentifier(binding) =
                        &declarator.id
                    {
                        if binding.name.as_str() == state_name {
                            declarator.init = None;
                        }
                    }
                }

                let init = std::mem::replace(
                    &mut **init_decl,
                    context.ast.variable_declaration(
                        SPAN,
                        oxc_ast::ast::VariableDeclarationKind::Var,
                        context.ast.vec(),
                        false,
                    ),
                );
                replacement_statements
                    .push(Statement::VariableDeclaration(context.ast.alloc(init)));
            }

            // Collect the linearized statements from the switch cases.
            let Statement::BlockStatement(body) = &mut for_statement.body else {
                continue;
            };
            let Statement::SwitchStatement(switch) = &mut body.body[0] else {
                continue;
            };

            for (case_index, stmt_indices) in &linearized {
                let case = &mut switch.cases[*case_index];
                for &stmt_index in stmt_indices {
                    if stmt_index < case.consequent.len() {
                        let mut stmt = std::mem::replace(
                            &mut case.consequent[stmt_index],
                            context.ast.statement_empty(SPAN),
                        );

                        // If the statement is a block that ends with a state
                        // assignment, strip the assignment and unwrap the block.
                        if let Statement::BlockStatement(block) = &mut stmt {
                            if let Some(last) = block.body.last() {
                                if extract_state_assignment(last, &state_name).is_some() {
                                    block.body.pop();
                                }
                            }
                            // Unwrap single-statement blocks.
                            if block.body.len() == 1 {
                                let inner = block.body.pop().unwrap();
                                replacement_statements.push(inner);
                                continue;
                            }
                            // Skip empty blocks entirely.
                            if block.body.is_empty() {
                                continue;
                            }
                        }

                        replacement_statements.push(stmt);
                    }
                }
            }

            // Replace the for statement with the linearized statements.
            let arena_replacements = context.ast.vec_from_iter(replacement_statements);
            operations::replace_statement_with_multiple(
                statements,
                index,
                arena_replacements,
                context,
            );
            changed = true;
        }

        changed
    }
}

/// A parsed switch case: either an exit (break) or a transition with statements.
#[derive(Debug, Clone)]
enum StateAction {
    /// Case ends with `break` or falls through to the break after the switch.
    Exit,
    /// Case has statements and transitions to another state.
    /// Contains the case index, the statement indices to extract (excluding
    /// the state assignment and continue), and the next state.
    Transition {
        case_index: usize,
        statement_indices: Vec<usize>,
        next_state: Option<i64>,
    },
    /// Case ends with a return/throw — terminal, no next state.
    Terminal {
        case_index: usize,
        statement_indices: Vec<usize>,
    },
}

/// Parse the switch statement into a map from state value to action.
///
/// The obfuscator generates case pairs: an empty decoy case followed by the
/// real case with the code. The real case's label is the actual state value.
/// We identify the "real" cases as those with non-empty consequent, and use
/// the LAST case label before the code as the state value (since the first
/// label in a pair is the decoy).
fn parse_state_map(
    switch: &oxc_ast::ast::SwitchStatement<'_>,
    state_name: &str,
) -> Option<Vec<(i64, StateAction)>> {
    let mut state_map: Vec<(i64, StateAction)> = Vec::new();

    // Track the label of the preceding empty (fall-through) case.
    // In the obfuscator pattern, case pairs look like:
    //   case REAL_STATE:   // empty, falls through
    //   case DECOY:        // has the code
    // The REAL_STATE label is the one used in state transitions.
    let mut pending_fallthrough_label: Option<i64> = None;

    for (case_index, case) in switch.cases.iter().enumerate() {
        let Some(test) = &case.test else {
            pending_fallthrough_label = None;
            continue;
        };

        let Some(label_value) = extract_numeric_value(test) else {
            pending_fallthrough_label = None;
            continue;
        };

        // If this case has an empty consequent, it falls through to the next.
        // Record its label as the real state value for the next case.
        if case.consequent.is_empty() {
            pending_fallthrough_label = Some(label_value);
            continue;
        }

        // This case has code. Both the fall-through label (if any) and
        // this case's own label should map to the same action.
        let fallthrough_label = pending_fallthrough_label.take();

        // Use the fall-through label as the primary state value (this is
        // how the obfuscator generates state transitions), but also register
        // the case's own label so transitions to either value work.
        let primary_label = fallthrough_label.unwrap_or(label_value);
        let secondary_label = if fallthrough_label.is_some() {
            Some(label_value)
        } else {
            None
        };

        // If the primary label is already mapped, use the secondary label
        // (if available) as the state value for this case's body.
        if state_map.iter().any(|(s, _)| *s == primary_label) {
            if let Some(secondary) = secondary_label {
                if !state_map.iter().any(|(s, _)| *s == secondary) {
                    // Fall through: use the secondary label as the state value
                    // and continue to parse this case's body below.
                } else {
                    continue;
                }
            } else {
                continue;
            }
        }

        // The state value is the first unused label.
        let state_value = if state_map.iter().any(|(s, _)| *s == primary_label) {
            secondary_label.unwrap()
        } else {
            primary_label
        };
        // Also register the other label as an alias.
        let alias_label = if state_value == primary_label {
            secondary_label
        } else {
            Some(primary_label)
        };

        // Check if this is a break case (exit).
        if case.consequent.len() == 1
            && matches!(&case.consequent[0], Statement::BreakStatement(_))
        {
            if let Some(alias) = alias_label {
                if !state_map.iter().any(|(s, _)| *s == alias) {
                    state_map.push((alias, StateAction::Exit));
                }
            }
            state_map.push((state_value, StateAction::Exit));
            continue;
        }

        // Parse the case body for state transitions and terminal statements.
        let mut statement_indices: Vec<usize> = Vec::new();
        let mut next_state: Option<i64> = None;
        let mut is_terminal = false;
        let mut is_exit = false;

        for (stmt_index, stmt) in case.consequent.iter().enumerate() {
            if matches!(stmt, Statement::ContinueStatement(_)) {
                break;
            }

            if let Some(assigned_state) = extract_state_assignment(stmt, state_name) {
                next_state = Some(assigned_state);
                continue;
            }

            // Check inside block statements for state assignments.
            // Pattern: `{ code; state = N; }` — extract the state assignment
            // and keep the block (its contents will be unwrapped later).
            if let Statement::BlockStatement(block) = stmt {
                if let Some(last) = block.body.last() {
                    if let Some(assigned_state) =
                        extract_state_assignment(last, state_name)
                    {
                        next_state = Some(assigned_state);
                        // Only keep the block if it has other statements besides
                        // the state assignment.
                        if block.body.len() > 1 {
                            statement_indices.push(stmt_index);
                        }
                        continue;
                    }
                }
            }

            if matches!(stmt, Statement::BreakStatement(_)) {
                is_exit = true;
                break;
            }

            if matches!(
                stmt,
                Statement::ReturnStatement(_) | Statement::ThrowStatement(_)
            ) {
                statement_indices.push(stmt_index);
                is_terminal = true;
                break;
            }

            if contains_return(stmt) {
                statement_indices.push(stmt_index);
                is_terminal = true;
                break;
            }

            statement_indices.push(stmt_index);
        }

        let action = if is_exit {
            Some(StateAction::Exit)
        } else if is_terminal {
            Some(StateAction::Terminal {
                case_index,
                statement_indices,
            })
        } else if next_state.is_some() || !statement_indices.is_empty() {
            Some(StateAction::Transition {
                case_index,
                statement_indices,
                next_state,
            })
        } else {
            None
        };

        if let Some(action) = action {
            // Register the alias label so state transitions to either value work.
            if let Some(alias) = alias_label {
                if !state_map.iter().any(|(s, _)| *s == alias) {
                    state_map.push((alias, action.clone()));
                }
            }
            state_map.push((state_value, action));
        }
    }

    if state_map.is_empty() {
        return None;
    }

    Some(state_map)
}

/// Trace through the state machine and return the ordered list of
/// (case_index, statement_indices) to extract.
fn trace_states(
    initial_state: i64,
    state_map: &[(i64, StateAction)],
) -> Option<Vec<(usize, Vec<usize>)>> {
    let mut result: Vec<(usize, Vec<usize>)> = Vec::new();
    let mut current_state = initial_state;
    let mut visited: Vec<i64> = Vec::new();

    loop {
        // Prevent infinite loops.
        if visited.contains(&current_state) {
            return None;
        }
        visited.push(current_state);

        let action = state_map.iter().find(|(s, _)| *s == current_state);
        let Some((_, action)) = action else {
            // Unknown state — can't linearize.
            return None;
        };

        match action {
            StateAction::Exit => {
                // Done — we've reached the exit state.
                break;
            }
            StateAction::Transition {
                case_index,
                statement_indices,
                next_state,
            } => {
                if !statement_indices.is_empty() {
                    result.push((*case_index, statement_indices.clone()));
                }
                if let Some(next) = next_state {
                    current_state = *next;
                } else {
                    // No explicit next state — falls through to break.
                    break;
                }
            }
            StateAction::Terminal {
                case_index,
                statement_indices,
            } => {
                result.push((*case_index, statement_indices.clone()));
                // Terminal — no next state.
                break;
            }
        }
    }

    Some(result)
}

/// Extract the initial state value from the for-init.
///
/// Handles: `var ..., state = N` or `state = N`.
fn extract_initial_state(
    init: &Option<oxc_ast::ast::ForStatementInit<'_>>,
    state_name: &str,
) -> Option<i64> {
    let init = init.as_ref()?;
    match init {
        oxc_ast::ast::ForStatementInit::VariableDeclaration(declaration) => {
            // Look for the state variable among the declarators.
            for declarator in &declaration.declarations {
                if let oxc_ast::ast::BindingPattern::BindingIdentifier(binding) = &declarator.id {
                    if binding.name.as_str() == state_name {
                        if let Some(init_expr) = &declarator.init {
                            return extract_numeric_value(init_expr);
                        }
                    }
                }
            }
            None
        }
        _ => None,
    }
}

/// Extract a state assignment: `state = N;` as an expression statement.
fn extract_state_assignment(statement: &Statement<'_>, state_name: &str) -> Option<i64> {
    let Statement::ExpressionStatement(expression_statement) = statement else {
        return None;
    };
    let Expression::AssignmentExpression(assignment) = &expression_statement.expression else {
        return None;
    };
    if assignment.operator != oxc_syntax::operator::AssignmentOperator::Assign {
        return None;
    }
    let oxc_ast::ast::AssignmentTarget::AssignmentTargetIdentifier(target) = &assignment.left
    else {
        return None;
    };
    if target.name.as_str() != state_name {
        return None;
    }
    extract_numeric_value(&assignment.right)
}

/// Extract a numeric value from an expression (literal or negated literal).
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

/// Check if a statement contains a return statement (including inside blocks).
fn contains_return(statement: &Statement<'_>) -> bool {
    match statement {
        Statement::ReturnStatement(_) | Statement::ThrowStatement(_) => true,
        Statement::BlockStatement(block) => block.body.iter().any(contains_return),
        Statement::IfStatement(if_stmt) => {
            contains_return(&if_stmt.consequent)
                || if_stmt
                    .alternate
                    .as_ref()
                    .map_or(false, |alt| contains_return(alt))
        }
        Statement::TryStatement(try_stmt) => {
            try_stmt.block.body.iter().any(contains_return)
                || try_stmt
                    .handler
                    .as_ref()
                    .map_or(false, |h| h.body.body.iter().any(contains_return))
                || try_stmt
                    .finalizer
                    .as_ref()
                    .map_or(false, |f| f.body.iter().any(contains_return))
        }
        _ => false,
    }
}
