//! Pass 1 — name resolution. Turns the LoweredAst ([`ast::LoweredProgram`]) into a
//! [`ResolvedProgram`] by assigning a unique [`BindingId`](super::analysed::BindingId) to every
//! `let` and lambda parameter and replacing each `String` name (`Var`, `FunctionInvocation`,
//! `Let`) with its id. No type logic lives here.
//!
//! Intended implementation:
//! - Carry a scope stack — a `Vec<(String, BindingId)>` searched from the back so the newest
//!   binding wins (shadowing) — and truncate it back to its entry length on leaving a `let`
//!   body or lambda. Each `let`/param mints a fresh `BindingId` (a monotonic counter) and pushes
//!   a [`ResolvedBinding`](super::resolved::ResolvedBinding) (name + span) into the table.
//! - **Pre-register all top-level `let` names before resolving their bodies.** That is the seam
//!   that later makes mutual recursion / forward references resolvable at the name level without
//!   touching pass 2.
//! - An unknown name is an [`AnalysisError::UnboundName`]. Collect all of them into the `Vec`
//!   rather than bailing on the first.

use std::collections::HashMap;
use std::rc::Rc;

use miette::SourceSpan;

use super::AnalysisError;
use super::resolved::ResolvedProgram;
use crate::analyse::analysed::BindingId;
use crate::analyse::resolved::{
    ResolvedBinding, ResolvedExpression, ResolvedExpressionKind, ResolvedLambda, ResolvedLiteral,
    ResolvedParam,
};
use crate::parse::ast::{self, Expression, ExpressionKind, Literal, Param};

/// Resolve every name in `program` to a [`BindingId`](super::analysed::BindingId).
pub(super) fn resolve(
    program: ast::LoweredProgram,
) -> Result<ResolvedProgram, Vec<AnalysisError>> {
    let mut bindings_arena = BindingArena::new();
    let mut scope = Scope::Empty;
    let mut expressions = Vec::new();
    let mut errors = Vec::new();

    // Names declared *in this block* (the top-level sequence), mapped to the first declaration's
    // span so a redeclaration error can point back at it. Block-local, unlike `scope`: it must not
    // leak into child blocks — a lambda body starts fresh, so a param may legitimately reuse an
    // outer name. It also can't be derived from `scope`, which contains outer bindings too, so a
    // `scope.lookup` would wrongly flag legal shadowing of an outer name as a redeclaration.
    // Owns its keys (`String`, not `&str`): the expressions are consumed below, so a borrow into
    // them couldn't outlive the loop iteration. One clone per top-level `let`.
    let mut declared: HashMap<String, SourceSpan> = HashMap::new();

    for expression in program.expressions {
        // The one block-level concern that isn't per-expression: flag a same-block redeclaration.
        // Resolution itself (including the `let` scope threading below) is uniform across kinds.
        if let ExpressionKind::Let { name, .. } = &expression.kind {
            match declared.get(name.as_str()) {
                Some(&original_span) => errors.push(AnalysisError::DuplicateBinding {
                    name: name.clone(),
                    span: expression.span,
                    original_span,
                }),
                None => {
                    declared.insert(name.clone(), expression.span);
                }
            }
        }

        // `resolve_expression` hands back the scope the next sibling sees; only a `let` changes it.
        let (result, next_scope) = resolve_expression(expression, &scope, &mut bindings_arena);
        scope = next_scope;
        match result {
            Ok(resolved) => expressions.push(resolved),
            Err(error) => errors.push(error),
        }
    }

    match errors.is_empty() {
        true => Ok(ResolvedProgram {
            expressions,
            bindings: bindings_arena.into_bindings(),
        }),
        false => Err(errors),
    }
}

/// The binding table under construction. A [`BindingId`] is exactly an index into this
/// arena, so id-minting lives here to keep that invariant local.
struct BindingArena(Vec<ResolvedBinding>);

impl BindingArena {
    fn new() -> BindingArena {
        BindingArena(Vec::new())
    }

    /// The id the *next* pushed binding will receive.
    fn mint_binding_id(&self) -> BindingId {
        BindingId(self.0.len())
    }

    fn push(&mut self, binding: ResolvedBinding) {
        self.0.push(binding);
    }

    /// Consume into the plain vec `ResolvedProgram` expects.
    fn into_bindings(self) -> Vec<ResolvedBinding> {
        self.0
    }
}

// Linked list of Rc backed Scope Entries
struct ScopeEntry {
    name: String,
    binding: BindingId,
    parent: Scope,
}

#[derive(Clone)]
enum Scope {
    Empty,
    Cons(Rc<ScopeEntry>),
}

impl Scope {
    fn extend(&self, name: String, binding: BindingId) -> Scope {
        // O(1): clones one Rc for the parent tail.
        Scope::Cons(Rc::new(ScopeEntry {
            name,
            binding,
            parent: self.clone(),
        }))
    }

    fn lookup(&self, name: &str) -> Option<BindingId> {
        let mut cur = self;
        while let Scope::Cons(entry) = cur {
            if entry.name == name {
                return Some(entry.binding); // newest-first walk ⇒ shadowing for free
            }
            cur = &entry.parent;
        }

        None
    }
}

/// Mint a fresh binding for `name`, record its name+span in the arena, and return the id together
/// with a scope extended by it. The single place a `let` enters the binding table.
fn bind_let(
    name: String,
    span: SourceSpan,
    scope: &Scope,
    bindings_arena: &mut BindingArena,
) -> (BindingId, Scope) {
    let binding = bindings_arena.mint_binding_id();
    // The name lives in two independent owners — the permanent arena record and the transient
    // scope entry — so one clone is unavoidable; move into the arena, clone into the scope.
    let extended = scope.extend(name.clone(), binding);
    bindings_arena.push(ResolvedBinding { name, span });
    (binding, extended)
}

/// Resolve one expression, returning both its resolved node (or error) and the scope the *next
/// sibling in a sequence* should see. Only a `let` returns an outgoing scope different from
/// `scope`; every other kind hands `scope` straight back. This is the single home of `let`
/// resolution — the driver's block loop threads the outgoing scope forward without special-casing.
fn resolve_expression(
    expr: Expression,
    scope: &Scope,
    bindings_arena: &mut BindingArena,
) -> (Result<ResolvedExpression, AnalysisError>, Scope) {
    let span = expr.span;

    match expr.kind {
        // A `let` mints a binding and hands an extended scope to its successors — even when its
        // value fails to resolve, so later references to the name don't cascade into spurious
        // `UnboundName`s.
        ExpressionKind::Let { name, value } => {
            let value = resolve_subexpr(*value, scope, bindings_arena);
            let (binding, extended) = bind_let(name, span, scope, bindings_arena);
            let node = value.map(|value| ResolvedExpression {
                kind: ResolvedExpressionKind::Let {
                    binding,
                    value: Box::new(value),
                },
                span,
            });
            (node, extended)
        }
        // Every other kind leaves the sibling scope untouched, so resolve to a plain result and
        // pair it with an unchanged `scope`.
        kind => (
            resolve_subexpr(Expression { kind, span }, scope, bindings_arena),
            scope.clone(),
        ),
    }
}

/// Resolve an expression whose outgoing scope is irrelevant — i.e. one in *expression position*
/// (an operand, a call argument, a lambda body), not a block sibling. Keeps `?` ergonomics while
/// leaving `let` scope-threading to the single caller in [`resolve_expression`].
fn resolve_subexpr(
    expr: Expression,
    scope: &Scope,
    bindings_arena: &mut BindingArena,
) -> Result<ResolvedExpression, AnalysisError> {
    let span = expr.span;
    let kind = match expr.kind {
        ExpressionKind::Var(string_identifier) => match scope.lookup(&string_identifier) {
            Some(binding) => ResolvedExpressionKind::Var(binding),
            None => {
                return Err(AnalysisError::UnboundName {
                    name: string_identifier,
                    span,
                });
            }
        },
        ExpressionKind::Literal(Literal::Int(v)) => {
            ResolvedExpressionKind::Literal(ResolvedLiteral::Int(v))
        }
        ExpressionKind::Literal(Literal::String(v)) => {
            ResolvedExpressionKind::Literal(ResolvedLiteral::String(v))
        }
        ExpressionKind::Literal(Literal::Bool(v)) => {
            ResolvedExpressionKind::Literal(ResolvedLiteral::Bool(v))
        }
        ExpressionKind::Literal(Literal::Float(v)) => {
            ResolvedExpressionKind::Literal(ResolvedLiteral::Float(v))
        }
        ExpressionKind::Add(lhs, rhs) => {
            let lhs = resolve_subexpr(*lhs, scope, bindings_arena)?;
            let rhs = resolve_subexpr(*rhs, scope, bindings_arena)?;
            ResolvedExpressionKind::Add(Box::new(lhs), Box::new(rhs))
        }
        ExpressionKind::Mul(lhs, rhs) => {
            let lhs = resolve_subexpr(*lhs, scope, bindings_arena)?;
            let rhs = resolve_subexpr(*rhs, scope, bindings_arena)?;
            ResolvedExpressionKind::Mul(Box::new(lhs), Box::new(rhs))
        }
        ExpressionKind::Lambda(lambda) => {
            let (parameter, updated_scope) = match lambda.parameter {
                Some(param) => {
                    let (resolved_param, extended) =
                        resolve_parameter(param, span, scope, bindings_arena);
                    (Some(resolved_param), extended)
                }
                None => (None, scope.clone()),
            };

            let body = resolve_subexpr(*lambda.body, &updated_scope, bindings_arena)?;

            ResolvedExpressionKind::Lambda(ResolvedLambda {
                body: Box::new(body),
                parameter,
                return_type: lambda.return_type,
            })
        }
        ExpressionKind::FunctionInvocation {
            function_name,
            expressions,
        } => {
            let binding = match scope.lookup(&function_name) {
                Some(binding) => binding,
                None => return Err(AnalysisError::UnboundName {
                    name: function_name,
                    span,
                }),
            };

            let arg_count = expressions.len();
            let resolved_args = expressions.into_iter().try_fold(
                Vec::with_capacity(arg_count),
                |mut resolved_args, argument| {
                    resolved_args.push(resolve_subexpr(argument, scope, bindings_arena)?);

                    Ok(resolved_args)
                },
            )?;

            ResolvedExpressionKind::FunctionInvocation(binding, resolved_args)
        }
        // A `let` here is in expression position: its binding has no following siblings to see it,
        // so the extended scope is discarded (only [`resolve_expression`] threads it to siblings).
        ExpressionKind::Let { name, value } => {
            let value = resolve_subexpr(*value, scope, bindings_arena)?;
            let (binding, _extended) = bind_let(name, span, scope, bindings_arena);
            ResolvedExpressionKind::Let {
                binding,
                value: Box::new(value),
            }
        }
    };

    Ok(ResolvedExpression { kind, span })
}

/// Resolve a lambda parameter: mint its `BindingId`, record it in the arena, and
/// return the resolved param together with the scope extended with the new binding.
fn resolve_parameter(
    param: Param,
    span: SourceSpan,
    scope: &Scope,
    bindings_arena: &mut BindingArena,
) -> (ResolvedParam, Scope) {
    let binding_id = bindings_arena.mint_binding_id();
    // Same two-owner situation as `bind_let`: clone the name into the scope, move it into the arena.
    let updated_scope = scope.extend(param.name.clone(), binding_id);
    bindings_arena.push(ResolvedBinding {
        name: param.name,
        span,
    });

    (
        ResolvedParam {
            binding: binding_id,
            type_dec: param.type_dec,
        },
        updated_scope,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolve_src(src: &str) -> Result<ResolvedProgram, Vec<AnalysisError>> {
        let program = crate::parse::parse(src).expect("test source should parse");
        resolve(program)
    }

    #[test]
    fn later_sibling_sees_earlier_let() {
        // `a` in the second expression must resolve to the first `let`'s binding (threading works).
        let resolved = resolve_src("let a = 1\na + 2").expect("should resolve");
        assert_eq!(resolved.bindings.len(), 1);
        assert_eq!(resolved.bindings[0].name, "a");
    }

    #[test]
    fn lambda_param_shadows_outer_binding_without_error() {
        // Outer `let x`, then a lambda whose param is also `x`. Different scopes → no duplicate, and
        // the body's `x` resolves to the param (newest-first), not the outer binding.
        let resolved = resolve_src("let x = 1\nlet f = (x:Int) => x")
            .expect("cross-scope shadowing is allowed");

        let ResolvedExpressionKind::Let { value, .. } = &resolved.expressions[1].kind else {
            panic!("expected the second top-level expression to be a let");
        };
        let ResolvedExpressionKind::Lambda(lambda) = &value.kind else {
            panic!("expected the let value to be a lambda");
        };
        let param = lambda.parameter.as_ref().expect("lambda has a parameter");
        let ResolvedExpressionKind::Var(body_binding) = &lambda.body.kind else {
            panic!("expected the lambda body to be a var");
        };
        assert_eq!(
            *body_binding, param.binding,
            "body `x` resolves to the param, not the outer `let x`"
        );
    }

    #[test]
    fn same_block_redeclaration_is_one_error() {
        let errors = resolve_src("let x = 1\nlet x = 2").expect_err("redeclaration is an error");
        assert_eq!(errors.len(), 1);
        assert!(matches!(
            errors[0],
            AnalysisError::DuplicateBinding { ref name, .. } if name == "x"
        ));
    }

    #[test]
    fn unbound_names_are_collected_not_bailed() {
        // Two sibling expressions, each an unbound name: both errors surface.
        let errors = resolve_src("foo\nbar").expect_err("unbound names are errors");
        assert_eq!(errors.len(), 2);
        assert!(
            errors
                .iter()
                .all(|error| matches!(error, AnalysisError::UnboundName { .. }))
        );
    }

    #[test]
    #[ignore = "needs block expressions — block-local `let` scoping"]
    fn block_local_let_is_not_visible_outside_the_block() {
        // A block's local binding is scoped to the block. Referencing `inner` after the
        // block closes must be an unbound name, not a leak of the block's scope.
        let errors = resolve_src("let outer = { let inner = 1  inner }\ninner")
            .expect_err("`inner` must not escape its block");
        assert!(matches!(errors[0], AnalysisError::UnboundName { .. }));
    }
}
