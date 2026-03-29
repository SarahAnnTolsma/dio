//! Statement pattern matching.

use oxc_ast::ast::Statement;

use super::capture::MatchResult;
use super::expression::ExpressionPattern;

/// A composable pattern that matches against JavaScript statement AST nodes.
#[derive(Clone)]
pub enum StatementPattern {
    // -- Meta-patterns --
    /// Matches any statement.
    Any,

    /// Matches the inner pattern and captures the matched statement.
    Capture(String, Box<StatementPattern>),

    /// Matches zero or more consecutive statements matching the inner pattern.
    /// Only meaningful inside `BlockStatement` patterns.
    Repeat(Box<StatementPattern>),

    /// All sub-patterns must match the same statement.
    And(Vec<StatementPattern>),

    /// At least one sub-pattern must match.
    Or(Vec<StatementPattern>),

    /// Matches if the inner pattern does NOT match.
    Not(Box<StatementPattern>),

    // -- Concrete matchers --
    /// Matches an expression statement containing the given expression pattern.
    ExpressionStatement(ExpressionPattern),

    /// Matches a return statement with an optional expression pattern for the return value.
    ReturnStatement(Option<ExpressionPattern>),

    /// Matches a block statement whose body matches the given statement patterns.
    /// `Repeat` patterns in the list match zero or more consecutive statements.
    BlockStatement(Vec<StatementPattern>),

    /// Matches an if statement with the given test, consequent, and optional alternate.
    IfStatement {
        /// Pattern for the test expression.
        test: ExpressionPattern,
        /// Pattern for the consequent (then) branch.
        consequent: Box<StatementPattern>,
        /// Pattern for the alternate (else) branch, if present.
        alternate: Option<Box<StatementPattern>>,
    },

    /// Matches a variable declaration.
    VariableDeclaration,

    /// Matches an empty statement (`;`).
    EmptyStatement,
}

impl StatementPattern {
    /// Match this pattern against a statement.
    pub fn match_statement(&self, statement: &Statement<'_>) -> MatchResult {
        match self {
            StatementPattern::Any => MatchResult::matched(),

            StatementPattern::Capture(_name, inner) => {
                // Capture for statements doesn't extract a value (statements don't have
                // a simple scalar representation), but it marks a successful match.
                inner.match_statement(statement)
            }

            StatementPattern::Repeat(_) => {
                // Repeat is only meaningful in the context of BlockStatement matching.
                // Matching a single statement against Repeat checks if it matches the inner.
                MatchResult::no_match()
            }

            StatementPattern::And(patterns) => {
                let mut combined = MatchResult::matched();
                for pattern in patterns {
                    let result = pattern.match_statement(statement);
                    if !result.matched {
                        return MatchResult::no_match();
                    }
                    combined.merge_captures(&result);
                }
                combined
            }

            StatementPattern::Or(patterns) => {
                for pattern in patterns {
                    let result = pattern.match_statement(statement);
                    if result.matched {
                        return result;
                    }
                }
                MatchResult::no_match()
            }

            StatementPattern::Not(inner) => {
                if inner.match_statement(statement).matched {
                    MatchResult::no_match()
                } else {
                    MatchResult::matched()
                }
            }

            StatementPattern::ExpressionStatement(expression_pattern) => {
                if let Statement::ExpressionStatement(expression_statement) = statement {
                    return expression_pattern.match_expression(&expression_statement.expression);
                }
                MatchResult::no_match()
            }

            StatementPattern::ReturnStatement(value_pattern) => {
                if let Statement::ReturnStatement(return_statement) = statement {
                    match (value_pattern, &return_statement.argument) {
                        (None, None) => return MatchResult::matched(),
                        (Some(pattern), Some(value)) => {
                            return pattern.match_expression(value);
                        }
                        _ => {}
                    }
                }
                MatchResult::no_match()
            }

            StatementPattern::BlockStatement(statement_patterns) => {
                if let Statement::BlockStatement(block) = statement {
                    return match_statement_list(&block.body, statement_patterns);
                }
                MatchResult::no_match()
            }

            StatementPattern::IfStatement {
                test,
                consequent,
                alternate,
            } => {
                if let Statement::IfStatement(if_statement) = statement {
                    let test_result = test.match_expression(&if_statement.test);
                    if !test_result.matched {
                        return MatchResult::no_match();
                    }

                    let consequent_result = consequent.match_statement(&if_statement.consequent);
                    if !consequent_result.matched {
                        return MatchResult::no_match();
                    }

                    // Check alternate.
                    match (alternate, &if_statement.alternate) {
                        (None, None) => {
                            let mut result = MatchResult::matched();
                            result.merge_captures(&test_result);
                            result.merge_captures(&consequent_result);
                            return result;
                        }
                        (Some(alt_pattern), Some(alt_statement)) => {
                            let alt_result = alt_pattern.match_statement(alt_statement);
                            if alt_result.matched {
                                let mut result = MatchResult::matched();
                                result.merge_captures(&test_result);
                                result.merge_captures(&consequent_result);
                                result.merge_captures(&alt_result);
                                return result;
                            }
                        }
                        _ => {}
                    }
                }
                MatchResult::no_match()
            }

            StatementPattern::VariableDeclaration => {
                if matches!(statement, Statement::VariableDeclaration(_)) {
                    MatchResult::matched()
                } else {
                    MatchResult::no_match()
                }
            }

            StatementPattern::EmptyStatement => {
                if matches!(statement, Statement::EmptyStatement(_)) {
                    MatchResult::matched()
                } else {
                    MatchResult::no_match()
                }
            }
        }
    }
}

/// Match a list of statements against a list of statement patterns.
/// Handles `Repeat` patterns that can match zero or more consecutive statements.
fn match_statement_list(
    statements: &[Statement<'_>],
    patterns: &[StatementPattern],
) -> MatchResult {
    let mut statement_index = 0;
    let mut pattern_index = 0;
    let mut combined = MatchResult::matched();

    while pattern_index < patterns.len() {
        let pattern = &patterns[pattern_index];

        if let StatementPattern::Repeat(inner) = pattern {
            // Greedy match: consume as many statements as possible that match inner.
            while statement_index < statements.len() {
                let result = inner.match_statement(&statements[statement_index]);
                if result.matched {
                    combined.merge_captures(&result);
                    statement_index += 1;
                } else {
                    break;
                }
            }
            pattern_index += 1;
        } else {
            // Regular pattern: must match exactly the next statement.
            if statement_index >= statements.len() {
                return MatchResult::no_match();
            }
            let result = pattern.match_statement(&statements[statement_index]);
            if !result.matched {
                return MatchResult::no_match();
            }
            combined.merge_captures(&result);
            statement_index += 1;
            pattern_index += 1;
        }
    }

    // All patterns consumed. Remaining statements are okay only if there are none.
    if statement_index == statements.len() {
        combined
    } else {
        MatchResult::no_match()
    }
}
