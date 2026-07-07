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
    ResolvedBinding, ResolvedExpression, ResolvedExpressionKind, ResolvedLambda, ResolvedParam,
};
use crate::parse::ast::{self, Expression, ExpressionKind, Param};

/// Resolve every name in `program` to a [`BindingId`](super::analysed::BindingId).
pub(super) fn resolve(
    program: &ast::LoweredProgram,
) -> Result<ResolvedProgram, Vec<AnalysisError>> {
    let mut state = ResolvingState::new();

    // Names declared *in this block* (the top-level sequence), mapped to the first declaration's
    // span so a redeclaration error can point back at it. A local, not a field of `ResolvingState`:
    // unlike `scope`, it must not leak into child blocks — a lambda body starts fresh, so a param
    // may legitimately reuse an outer name.
    let mut declared: HashMap<&str, SourceSpan> = HashMap::new();

    for expression in &program.expressions {
        match &expression.kind {
            // A block-level `let` threads its binding into the scope seen by later siblings, so it
            // is handled here rather than in `resolve_expression` (which cannot return a new scope).
            ExpressionKind::Let { name, value } => {
                if let Some(&original_span) = declared.get(name.as_str()) {
                    state.errors.push(AnalysisError::DuplicateBinding {
                        name: name.clone(),
                        span: expression.span,
                        original_span,
                    });
                } else {
                    declared.insert(name.as_str(), expression.span);
                }

                // Value is resolved *before* the binding enters scope (non-recursive let).
                let value = resolve_expression(value, &state.scope, &mut state.bindings_arena);

                // Mint + extend even if the value failed, so later references to this name don't
                // cascade into spurious `UnboundName`s; only emit the node when the value resolved.
                let (binding, extended) = bind_let(
                    name,
                    expression.span,
                    &state.scope,
                    &mut state.bindings_arena,
                );
                state.scope = extended;

                match value {
                    Ok(value) => state.expressions.push(ResolvedExpression {
                        kind: ResolvedExpressionKind::Let {
                            binding,
                            value: Box::new(value),
                        },
                        span: expression.span,
                    }),
                    Err(error) => state.errors.push(error),
                }
            }
            _ => match resolve_expression(expression, &state.scope, &mut state.bindings_arena) {
                Ok(resolved) => state.expressions.push(resolved),
                Err(error) => state.errors.push(error),
            },
        }
    }

    match state.errors.is_empty() {
        true => Ok(ResolvedProgram {
            expressions: state.expressions,
            bindings: state.bindings_arena.into_bindings(),
        }),
        false => Err(state.errors),
    }
}

struct ResolvingState {
    bindings_arena: BindingArena,
    scope: Scope,
    expressions: Vec<ResolvedExpression>,
    errors: Vec<AnalysisError>,
}

impl ResolvingState {
    fn new() -> ResolvingState {
        ResolvingState {
            expressions: Vec::new(),
            bindings_arena: BindingArena::new(),
            scope: Scope::Empty,
            errors: Vec::new(),
        }
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
    name: &str,
    span: SourceSpan,
    scope: &Scope,
    bindings_arena: &mut BindingArena,
) -> (BindingId, Scope) {
    let binding = bindings_arena.mint_binding_id();
    bindings_arena.push(ResolvedBinding {
        name: name.to_string(),
        span,
    });
    (binding, scope.extend(name.to_string(), binding))
}

fn resolve_expression(
    expr: &Expression,
    scope: &Scope,
    bindings_arena: &mut BindingArena,
) -> Result<ResolvedExpression, AnalysisError> {
    let (resvoled_expression_kind, additional_bindings) = match &expr.kind {
        ExpressionKind::Var(string_identifier) => {
            let lookup = scope.lookup(string_identifier);
            match lookup {
                Some(binding) => (ResolvedExpressionKind::Var(binding), None),
                None => {
                    return Err(AnalysisError::UnboundName {
                        name: string_identifier.to_string(),
                        span: expr.span,
                    });
                }
            }
        }
        ExpressionKind::Int(v) => (ResolvedExpressionKind::Int(*v), None),
        ExpressionKind::Add(lhs, rhs) => {
            let lhs = resolve_expression(&lhs, scope, bindings_arena)?;
            let rhs = resolve_expression(&rhs, scope, bindings_arena)?;

            (
                ResolvedExpressionKind::Add(Box::new(lhs), Box::new(rhs)),
                None,
            )
        }
        ExpressionKind::Mul(lhs, rhs) => {
            let lhs = resolve_expression(&lhs, scope, bindings_arena)?;
            let rhs = resolve_expression(&rhs, scope, bindings_arena)?;

            (
                ResolvedExpressionKind::Mul(Box::new(lhs), Box::new(rhs)),
                None,
            )
        }
        ExpressionKind::Lambda(lambda) => {
            let (parameter, updated_scope) = match &lambda.parameter {
                Some(param) => {
                    let (resolved_param, extended) =
                        resolve_parameter(param, expr.span, scope, bindings_arena);
                    (Some(resolved_param), extended)
                }
                None => (None, scope.clone()),
            };

            let body = resolve_expression(&lambda.body, &updated_scope, bindings_arena)?;

            (
                ResolvedExpressionKind::Lambda(ResolvedLambda {
                    body: Box::new(body),
                    parameter,
                    return_type: lambda.return_type.clone(),
                }),
                None,
            )
        }
        ExpressionKind::FunctionInvocation {
            function_name,
            expressions,
        } => {
            let binding =
                scope
                    .lookup(function_name)
                    .ok_or_else(|| AnalysisError::UnboundName {
                        name: function_name.clone(),
                        span: expr.span,
                    })?;

            let resolved_args = expressions.iter().try_fold(
                Vec::with_capacity(expressions.len()),
                |mut resolved_args, argument| {
                    resolved_args.push(resolve_expression(argument, scope, bindings_arena)?);

                    Ok(resolved_args)
                },
            )?;

            (
                ResolvedExpressionKind::FunctionInvocation(binding, resolved_args),
                None,
            )
        }
        // A `let` in expression position (not a block sequence) has no following siblings to see its
        // binding, so it needs no scope threading or duplicate-tracking.
        ExpressionKind::Let { name, value } => {
            let value = resolve_expression(value, scope, bindings_arena)?;
            let (binding, _extended) = bind_let(name, expr.span, scope, bindings_arena);
            (
                ResolvedExpressionKind::Let {
                    binding,
                    value: Box::new(value),
                },
                None,
            )
        }
    };

    let expression = ResolvedExpression {
        kind: resvoled_expression_kind,
        span: expr.span,
    };

    if let Some(binding) = additional_bindings {
        bindings_arena.push(binding);
    }

    Ok(expression)
}

/// Resolve a lambda parameter: mint its `BindingId`, record it in the arena, and
/// return the resolved param together with the scope extended with the new binding.
fn resolve_parameter(
    param: &Param,
    span: SourceSpan,
    scope: &Scope,
    bindings_arena: &mut BindingArena,
) -> (ResolvedParam, Scope) {
    let binding_id = bindings_arena.mint_binding_id();
    bindings_arena.push(ResolvedBinding {
        name: param.name.clone(),
        span,
    });
    let updated_scope = scope.extend(param.name.clone(), binding_id);

    (
        ResolvedParam {
            binding: binding_id,
            type_dec: param.type_dec.clone(),
        },
        updated_scope,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolve_src(src: &str) -> Result<ResolvedProgram, Vec<AnalysisError>> {
        let program = crate::parse::parse(src).expect("test source should parse");
        resolve(&program)
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
        let resolved =
            resolve_src("let x = 1\nlet f = (x) => x").expect("cross-scope shadowing is allowed");

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
}
