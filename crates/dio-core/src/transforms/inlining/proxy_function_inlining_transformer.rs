//! Inlines proxy functions that simply wrap another operation.
//!
//! JavaScript obfuscators (notably `javascript-obfuscator`) generate small
//! "proxy" functions that forward to a single operator or call. These add
//! indirection without changing semantics, making the code harder to read.
//!
//! # Supported proxy patterns
//!
//! **Binary operation proxy** — wraps a single binary operator:
//! ```js
//! function _0x1(a, b) { return a + b; }
//! _0x1(1, 2)  // inlined to: 1 + 2
//! ```
//!
//! **Call forwarding proxy** — forwards a call to its first argument:
//! ```js
//! function _0x4(a, b) { return a(b); }
//! _0x4(fn, arg)  // inlined to: fn(arg)
//! ```
//!
//! **Identity proxy** — returns its argument unchanged:
//! ```js
//! function _0x5(a) { return a; }
//! _0x5(x)  // inlined to: x
//! ```
//!
//! # Planned algorithm
//!
//! 1. **Identify proxy functions.** Walk all function declarations and function
//!    expressions looking for functions whose body consists of a single return
//!    statement. The return expression must only reference the function's own
//!    parameters (no free variables, no side-effecting operations beyond the
//!    single operator or call).
//!
//! 2. **Classify the proxy kind.** Based on the return expression:
//!    - `BinaryExpression` where both operands are parameter references
//!      → binary proxy (captures the operator).
//!    - `CallExpression` where the callee is a parameter reference and all
//!      arguments are parameter references → call forwarding proxy.
//!    - `Identifier` referencing a single parameter → identity proxy.
//!
//! 3. **Verify safe usage via scoping.** Use oxc's semantic bindings to find
//!    every reference to the proxy function's binding. All references must be
//!    call expressions where the proxy is the callee (not passed as a value,
//!    not reassigned, not used in typeof, etc.). If any reference is not a
//!    direct call, skip inlining for that function.
//!
//! 4. **Inline each call site.** For every verified call site:
//!    - Binary proxy: replace the `CallExpression` with a `BinaryExpression`
//!      whose operator is the captured operator and whose operands are the
//!      corresponding arguments from the call, using
//!      `operations::replace_expression`.
//!    - Call forwarding proxy: replace the `CallExpression` with a new
//!      `CallExpression` where the callee is the first argument and the
//!      remaining arguments are forwarded, using
//!      `operations::replace_expression`.
//!    - Identity proxy: replace the `CallExpression` with the single argument,
//!      using `operations::replace_expression`.
//!
//! 5. **Remove the proxy declaration.** After all call sites are inlined, the
//!    proxy function declaration is dead code. Remove it using
//!    `operations::remove_statement`.
//!
//! # Phase and priority
//!
//! This transformer runs in the **Main** phase with **First** priority. Proxy
//! function inlining should happen early because it exposes simpler expressions
//! (binary operations, direct calls) that downstream transformers like constant
//! folding and dead code elimination can act on.

use oxc_allocator::Vec as ArenaVec;
use oxc_ast::ast::Statement;
use oxc_traverse::TraverseCtx;

use crate::transformer::{AstNodeType, Transformer, TransformerPhase, TransformerPriority};

/// Inlines proxy functions that wrap a single binary operation, call, or identity.
///
/// See the [module-level documentation](self) for supported patterns and the
/// planned algorithm.
pub struct ProxyFunctionInliningTransformer;

impl Transformer for ProxyFunctionInliningTransformer {
    fn name(&self) -> &str {
        "ProxyFunctionInliningTransformer"
    }

    fn interests(&self) -> &[AstNodeType] {
        // Needs the full statement list to scan for proxy function declarations
        // and their call sites in a single pass.
        &[AstNodeType::StatementList]
    }

    fn priority(&self) -> TransformerPriority {
        TransformerPriority::First
    }

    fn phase(&self) -> TransformerPhase {
        TransformerPhase::Main
    }

    fn enter_statements<'a>(
        &self,
        _statements: &mut ArenaVec<'a, Statement<'a>>,
        _context: &mut TraverseCtx<'a, ()>,
    ) -> bool {
        // TODO: Implement proxy function inlining.
        // Steps:
        // 1. Scan statements for function declarations matching proxy patterns
        //    (single return statement, body only references parameters).
        // 2. Classify each proxy (binary, call forwarding, or identity).
        // 3. Use scoping to find all call sites and verify the function is only
        //    used as a callee.
        // 4. Inline each call site using operations::replace_expression.
        // 5. Remove the proxy declaration using operations::remove_statement.
        false
    }
}
