//! Binding resolution — name resolution. Turns the parsed AST ([`ast::ParsedProgram`]) into a
//! [`BindingResolvedProgram`] by assigning a unique [`BindingId`] to every `let` and lambda
//! parameter and replacing each `String` name (`Var`, `FunctionInvocation`, `Let`) with its id. No
//! type logic lives here.
//!
//! Intended implementation:
//! - Carry a scope stack — a `Vec<(String, BindingId)>` searched from the back so the newest
//!   binding wins (shadowing) — and truncate it back to its entry length on leaving a `let`
//!   body or lambda. Each `let`/param mints a fresh `BindingId` (a monotonic counter) and pushes
//!   a [`ResolvedBinding`] (name + span) into the table.
//! - **Pre-register all top-level `let` names before resolving their bodies.** That is the seam
//!   that later makes mutual recursion / forward references resolvable at the name level without
//!   touching type checking.
//! - An unknown name is a [`BindingResolutionError::UnboundName`]. Collect all of them into the
//!   `Vec` rather than bailing on the first.

mod binding_arena;
pub mod binding_resolved;
mod error;
mod scope;

use std::collections::{BTreeMap, HashMap};

use miette::SourceSpan;

use crate::{
    binding_resolution::{
        binding_arena::TypeBindingArea,
        binding_resolved::{ResolvedTypeExpression, ResolvedTypeExpressionKind},
        scope::{Scope, TypeBindingScopeEntry, VariableBindingScopeEntry},
    },
    parse::ast::{self, Expression, ExpressionKind, Literal, Param},
    prelude,
};

pub use binding_resolved::{
    BindingId, BindingResolvedProgram, ResolvedBinding, ResolvedExpression, ResolvedExpressionKind,
    ResolvedLambda, ResolvedLiteral, ResolvedParam,
};
pub use error::BindingResolutionError;

use binding_arena::VariableBindingArena;

struct ResolveContext {
    var_binding_arena: VariableBindingArena,
    type_binding_arena: TypeBindingArea,
}

/// Resolve every name in `program` to a [`BindingId`].
pub fn resolve(
    program: ast::ParsedProgram,
) -> Result<BindingResolvedProgram, Vec<BindingResolutionError>> {
    let mut resolve_context = ResolveContext {
        type_binding_arena: TypeBindingArea::new(),
        var_binding_arena: VariableBindingArena::new(),
    };

    // The prelude occupies `TypeBindingId(0..PRELUDE_TYPES.len())`: seeded before any user
    // declaration, so the ids line up positionally with `prelude::PRELUDE_TYPES` — that index is
    // how type checking recovers each builtin's `Type`, which this pass never sees. A user
    // `type Int = …` needs no special case: it mints a fresh id and shadows the prelude entry
    // through the ordinary scope chain.
    let mut scope = prelude::type_names().fold(Scope::new(), |scope, name| {
        let binding = resolve_context.type_binding_arena.extend(ResolvedBinding {
            name: name.to_string(),
            span: prelude::prelude_span(),
        });

        scope.extend_type(TypeBindingScopeEntry {
            name: name.to_string(),
            binding,
        })
    });
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
                Some(&original_span) => errors.push(BindingResolutionError::DuplicateBinding {
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
        // Collect the error and carry on rather than bailing, so every unbound name surfaces.
        match resolve_expression(expression, &scope, &mut resolve_context) {
            Ok((resolved, next_scope)) => {
                expressions.push(resolved);
                scope = next_scope;
            }
            Err(error) => errors.push(error),
        }
    }

    match errors.is_empty() {
        true => Ok(BindingResolvedProgram {
            expressions,
            bindings: resolve_context.var_binding_arena.into_vec(),
            type_bindings: resolve_context.type_binding_arena.into_vec(),
        }),
        false => Err(errors),
    }
}

/// Mint a fresh binding for `name`, record its name+span in the arena, and return the id together
/// with a scope extended by it. The single place a `let` enters the binding table.
fn bind_let(
    name: String,
    span: SourceSpan,
    scope: &Scope,
    ctx: &mut ResolveContext,
) -> (BindingId, Scope) {
    let binding = ctx.var_binding_arena.extend(ResolvedBinding {
        name: name.clone(),
        span,
    });

    let extended = scope.extend_variable(VariableBindingScopeEntry {
        name: name,
        binding,
    });

    (binding, extended)
}

fn resolve_expression(
    expr: Expression,
    scope: &Scope,
    ctx: &mut ResolveContext,
) -> Result<(ResolvedExpression, Scope), BindingResolutionError> {
    let span = expr.span;

    // Only a declaration arm replaces this. Cloning once up front is two `Rc` bumps — cheaper than
    // repeating `scope.clone()` in the nine arms that leave the sibling scope untouched.
    let mut outgoing_scope = scope.clone();

    let kind = match expr.kind {
        ExpressionKind::Var(string_identifier) => match scope.lookup(&string_identifier) {
            Some(binding) => ResolvedExpressionKind::Var(binding),
            None => {
                return Err(BindingResolutionError::UnboundName {
                    name: string_identifier,
                    span,
                });
            }
        },
        ExpressionKind::Literal(Literal::Unit) => {
            ResolvedExpressionKind::Literal(ResolvedLiteral::Unit)
        }
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
        // Name resolution is uniform across operators — the `BinaryOp` tag just passes through.
        ExpressionKind::Binary(op, lhs, rhs) => {
            let (lhs, _) = resolve_expression(*lhs, scope, ctx)?;
            let (rhs, _) = resolve_expression(*rhs, scope, ctx)?;
            ResolvedExpressionKind::Binary(op, Box::new(lhs), Box::new(rhs))
        }
        // Same as `Binary`: the `UnaryOp` tag just passes through resolution.
        ExpressionKind::Unary(op, operand) => {
            let (operand, _) = resolve_expression(*operand, scope, ctx)?;
            ResolvedExpressionKind::Unary(op, Box::new(operand))
        }
        ExpressionKind::Lambda(lambda) => {
            let (parameter, updated_scope) = match lambda.parameter {
                Some(param) => {
                    let (resolved_param, extended) = resolve_parameter(param, span, scope, ctx)?;
                    (Some(resolved_param), extended)
                }
                None => (None, scope.clone()),
            };

            let (body, _) = resolve_expression(*lambda.body, &updated_scope, ctx)?;

            ResolvedExpressionKind::Lambda(ResolvedLambda {
                body: Box::new(body),
                parameter,
                return_type: lambda
                    .return_type
                    .map(|type_expression| resolve_type_expression(type_expression, scope, ctx))
                    .transpose()?
                    .map(|(type_expr, _)| type_expr),
            })
        }
        ExpressionKind::FunctionInvocation {
            function,
            arguments,
        } => {
            let (resolved_function, scope) = resolve_expression(*function, scope, ctx)?;

            let arg_count = arguments.len();
            let resolved_args = arguments.into_iter().try_fold(
                Vec::with_capacity(arg_count),
                |mut resolved_args, argument| {
                    let (argument, _) = resolve_expression(argument, &scope, ctx)?;
                    resolved_args.push(argument);

                    Ok(resolved_args)
                },
            )?;

            ResolvedExpressionKind::FunctionInvocation {
                function: Box::new(resolved_function),
                arguments: resolved_args,
            }
        }
        ExpressionKind::Let {
            name,
            type_dec,
            value,
        } => {
            // `unzip` splits the `Option<(_, Scope)>` into its two halves so only the scope needs
            // a fallback. Any name the annotation introduces (a future `{ name: T, ...rest }`) is
            // scoped to the annotation and the value: `bind_let` below extends the *incoming*
            // `scope`, so those names can't ride `outgoing_scope` out to later siblings.
            let (optional_resolved_type_dec, annotation_scope) = type_dec
                .map(|type_dec| resolve_type_expression(type_dec, scope, ctx))
                .transpose()?
                .unzip();
            let updated_scope = annotation_scope.unwrap_or_else(|| scope.clone());
            let (value, _) = resolve_expression(*value, &updated_scope, ctx)?;
            let (binding, updated_scope) = bind_let(name, span, &updated_scope, ctx);
            outgoing_scope = updated_scope;
            ResolvedExpressionKind::Let {
                binding,
                type_dec: optional_resolved_type_dec,
                value: Box::new(value),
            }
        }
        // A block's elements are siblings, so thread the outgoing scope forward from one to the
        // next (as the top-level driver does) — a block-local `let` is then visible to later
        // siblings. The threaded scope is dropped at the closing brace, so it doesn't leak out.
        ExpressionKind::Block(expressions) => {
            let element_count = expressions.len();
            let (_scope, resolved) = expressions.into_iter().try_fold(
                (scope.clone(), Vec::with_capacity(element_count)),
                |(scope, mut resolved), expr| {
                    let (element, next_scope) = resolve_expression(expr, &scope, ctx)?;
                    resolved.push(element);
                    Ok((next_scope, resolved))
                },
            )?;
            ResolvedExpressionKind::Block(resolved)
        }
        // Each of the three parts is in expression position — a binding made inside a branch is
        // scoped to that branch, so none of their outgoing scopes escape.
        ExpressionKind::If {
            condition,
            true_pathway,
            false_pathway,
        } => {
            let (condition, _) = resolve_expression(*condition, scope, ctx)?;
            let (true_condition, _) = resolve_expression(*true_pathway, scope, ctx)?;
            let false_condition = match false_pathway {
                Some(false_pathway) => {
                    let (resolved, _) = resolve_expression(*false_pathway, scope, ctx)?;
                    Some(Box::new(resolved))
                }
                None => None,
            };

            ResolvedExpressionKind::If {
                condition: Box::new(condition),
                true_condition: Box::new(true_condition),
                false_condition,
            }
        }
        // Next up: mint a `TypeBindingId` for the identifier and set
        // `outgoing_scope = scope.extend_type(…)`. The arm belongs in this function precisely
        // because that scope extension is a type declaration's *only* effect — resolving it
        // anywhere the outgoing scope is discarded would be a guaranteed no-op.
        ExpressionKind::TypeDeclaration {
            identifier,
            type_expression,
        } => {
            // Register
            let (resolved_type_expression, scope) =
                resolve_type_expression(type_expression, scope, ctx)?;

            let binding = ctx.type_binding_arena.extend(ResolvedBinding {
                name: identifier.clone(),
                span,
            });

            outgoing_scope = scope.extend_type(TypeBindingScopeEntry {
                name: identifier,
                binding,
            });

            ResolvedExpressionKind::TypeDeclaration {
                identifier: binding,
                type_expression: resolved_type_expression,
            }
        }
        ExpressionKind::FieldAccess { target, identifier } => todo!(),
    };

    Ok((ResolvedExpression { kind, span }, outgoing_scope))
}

/// `span` is the *enclosing* expression's span: an [`ast::TypeExpression`] carries none of its own,
/// so an unbound type name can only be labelled at the declaration that mentions it.
///
/// The returned [`Scope`] is currently always the input unchanged — a type expression today only
/// *looks names up*. It is not dead weight: the planned row syntax
/// (`function rename(rec: { name: T, ...rest }): String`) introduces `T` and `rest` by occurrence
/// inside the type expression, with no declaration-level binder list to hang them on, and those
/// names have to reach the rest of the signature. Callers must keep that scope inside the
/// declaration — see the `Let` arm of [`resolve_expression`].
fn resolve_type_expression(
    type_expression: ast::TypeExpression,
    scope: &Scope,
    ctx: &mut ResolveContext,
) -> Result<(ResolvedTypeExpression, Scope), BindingResolutionError> {
    match type_expression.kind {
        ast::TypeExpressionKind::Named(name) => {
            // Lookup the name in the scope.
            match scope.lookup_type(&name) {
                Some(some_type) => Ok((
                    ResolvedTypeExpression {
                        kind: ResolvedTypeExpressionKind::Named(some_type),
                        span: type_expression.span,
                    },
                    scope.clone(),
                )),
                None => Err(BindingResolutionError::UnboundTypeName {
                    name,
                    span: type_expression.span,
                }),
            }
        }
        // Fields are threaded left to right rather than resolved against one fixed scope, so a
        // name introduced by an earlier field is visible to a later one — `{ a: T, b: T }` must
        // give both `T`s the same binding. Inert today (nothing introduces a name yet); load
        // bearing once `...rest` does. The keys carry over untouched — only values get resolved.
        ast::TypeExpressionKind::Record(fields) => {
            let (resolved, scope) = fields.into_iter().try_fold(
                (BTreeMap::new(), scope.clone()),
                |(mut acc, scope), (key, value)| {
                    let (value, scope) = resolve_type_expression(value, &scope, ctx)?;

                    acc.insert(key, value);

                    Ok((acc, scope))
                },
            )?;

            Ok((
                ResolvedTypeExpression {
                    kind: ResolvedTypeExpressionKind::Record(resolved),
                    span: type_expression.span,
                },
                scope,
            ))
        }
    }
}

/// Resolve a lambda parameter: mint its `BindingId`, record it in the arena, and
/// return the resolved param together with the scope extended with the new binding.
fn resolve_parameter(
    param: Param,
    span: SourceSpan,
    scope: &Scope,
    ctx: &mut ResolveContext,
) -> Result<(ResolvedParam, Scope), BindingResolutionError> {
    let binding_id = ctx.var_binding_arena.extend(ResolvedBinding {
        name: param.name.clone(),
        span,
    });

    // Same two-owner situation as `bind_let`: clone the name into the scope, move it into the arena.
    let updated_scope = scope.extend_variable(VariableBindingScopeEntry {
        name: param.name,
        binding: binding_id,
    });

    Ok((
        ResolvedParam {
            binding: binding_id,
            type_dec: param
                .type_dec
                .map(|type_expression| resolve_type_expression(type_expression, scope, ctx))
                .transpose()?
                .map(|(type_expr, _)| type_expr),
        },
        updated_scope,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binding_resolution::binding_resolved::TypeBindingId;

    fn resolve_src(src: &str) -> Result<BindingResolvedProgram, Vec<BindingResolutionError>> {
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
    fn an_annotation_does_not_disturb_the_sibling_scope() {
        // The `let`'s outgoing scope is built from the *incoming* one, not from the annotation's
        // scope — so an annotation can't contribute bindings to later siblings. Inert while type
        // expressions only look names up; the guard that matters once `{ name: T, ...rest }`
        // introduces `T` by occurrence and `T` must not outlive the annotation.
        let resolved = resolve_src("let a: Int = 1\na + 2").expect("should resolve");
        assert_eq!(resolved.bindings.len(), 1);
        assert_eq!(resolved.bindings[0].name, "a");
        assert_eq!(
            resolved.type_bindings.len(),
            prelude::PRELUDE_TYPES.len(),
            "an annotation mentioning a prelude type mints no new type binding"
        );
    }

    #[test]
    fn same_block_redeclaration_is_one_error() {
        let errors = resolve_src("let x = 1\nlet x = 2").expect_err("redeclaration is an error");
        assert_eq!(errors.len(), 1);
        assert!(matches!(
            errors[0],
            BindingResolutionError::DuplicateBinding { ref name, .. } if name == "x"
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
                .all(|error| matches!(error, BindingResolutionError::UnboundName { .. }))
        );
    }

    #[test]
    fn unknown_type_name_is_an_unbound_type_error() {
        // The type namespace holds the prelude and whatever the program declares, so a name that is
        // neither is unbound — and reports as a *type* error, distinct from an unbound value.
        let errors =
            resolve_src("type Alias = Missing").expect_err("unknown type name is an error");
        assert!(matches!(
            errors[0],
            BindingResolutionError::UnboundTypeName { ref name, .. } if name == "Missing"
        ));
    }

    #[test]
    fn a_builtin_resolves_to_its_prelude_binding() {
        // The contract type checking depends on: the prelude is seeded first, so a builtin's
        // `TypeBindingId` is its index in `prelude::PRELUDE_TYPES` — the index type checking uses
        // to look the `Type` back up. `Int` is entry 0.
        let resolved = resolve_src("type Alias = Int").expect("a builtin type name resolves");

        let int_id = TypeBindingId(
            prelude::type_names()
                .position(|name| name == "Int")
                .expect("`Int` is a prelude type"),
        );
        assert_eq!(resolved.type_bindings[int_id.0].name, "Int");

        let ResolvedExpressionKind::TypeDeclaration {
            type_expression, ..
        } = &resolved.expressions[0].kind
        else {
            panic!("expected a type declaration");
        };
        assert_eq!(
            type_expression.kind,
            ResolvedTypeExpressionKind::Named(int_id)
        );
    }

    #[test]
    fn a_user_declaration_shadows_a_prelude_type() {
        // Redeclaring a builtin is legal and needs no special case: it mints a fresh id and wins
        // the newest-first scope lookup, leaving the prelude's own binding untouched.
        let resolved = resolve_src("type Int = Float\ntype Alias = Int")
            .expect("shadowing a prelude type resolves");

        let ResolvedExpressionKind::TypeDeclaration { identifier, .. } =
            &resolved.expressions[0].kind
        else {
            panic!("expected a type declaration");
        };
        let ResolvedExpressionKind::TypeDeclaration {
            type_expression, ..
        } = &resolved.expressions[1].kind
        else {
            panic!("expected a type declaration");
        };

        assert!(
            identifier.0 >= prelude::PRELUDE_TYPES.len(),
            "the shadowing declaration mints an id past the prelude's"
        );
        assert_eq!(
            type_expression.kind,
            ResolvedTypeExpressionKind::Named(*identifier),
            "`Int` on the right resolves to the user's declaration, not the prelude's"
        );
    }

    #[test]
    #[ignore = "needs block expressions — block-local `let` scoping"]
    fn block_local_let_is_not_visible_outside_the_block() {
        // A block's local binding is scoped to the block. Referencing `inner` after the
        // block closes must be an unbound name, not a leak of the block's scope.
        let errors = resolve_src("let outer = { let inner = 1  inner }\ninner")
            .expect_err("`inner` must not escape its block");
        assert!(matches!(
            errors[0],
            BindingResolutionError::UnboundName { .. }
        ));
    }
}
