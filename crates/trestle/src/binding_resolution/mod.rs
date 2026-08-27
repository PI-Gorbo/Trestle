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

pub fn resolve(
    program: ast::ParsedProgram,
) -> Result<BindingResolvedProgram, Vec<BindingResolutionError>> {
    let mut resolve_context = ResolveContext {
        type_binding_arena: TypeBindingArea::new(),
        var_binding_arena: VariableBindingArena::new(),
    };

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

    let mut declared: HashMap<String, SourceSpan> = HashMap::new();

    for expression in program.expressions {
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
        ExpressionKind::Literal(Literal::Record(map)) => {
            let resolved_map =
                map.into_iter()
                    .try_fold(BTreeMap::new(), |mut state, (key, value)| {
                        let (resolved_expression, _) = resolve_expression(value, scope, ctx)?;

                        state.insert(key, resolved_expression);

                        Ok(state)
                    })?;

            ResolvedExpressionKind::Literal(ResolvedLiteral::Record(resolved_map))
        }
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

        ExpressionKind::Binary(op, lhs, rhs) => {
            let (lhs, _) = resolve_expression(*lhs, scope, ctx)?;
            let (rhs, _) = resolve_expression(*rhs, scope, ctx)?;
            ResolvedExpressionKind::Binary(op, Box::new(lhs), Box::new(rhs))
        }

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

        ExpressionKind::TypeDeclaration {
            identifier,
            type_expression,
        } => {
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
        ExpressionKind::FieldAccess { target, identifier } => {
            let (target, _) = resolve_expression(*target, scope, ctx)?;

            ResolvedExpressionKind::FieldAccess {
                target: Box::new(target),
                field_name: identifier,
            }
        }
    };

    Ok((ResolvedExpression { kind, span }, outgoing_scope))
}

fn resolve_type_expression(
    type_expression: ast::TypeExpression,
    scope: &Scope,
    ctx: &mut ResolveContext,
) -> Result<(ResolvedTypeExpression, Scope), BindingResolutionError> {
    match type_expression.kind {
        ast::TypeExpressionKind::Named(name) => match scope.lookup_type(&name) {
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
        },

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
        ast::TypeExpressionKind::Lambda(lambda_type_expression) => todo!(),
    }
}

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
        let resolved = resolve_src("let a = 1\na + 2").expect("should resolve");
        assert_eq!(resolved.bindings.len(), 1);
        assert_eq!(resolved.bindings[0].name, "a");
    }

    #[test]
    fn lambda_param_shadows_outer_binding_without_error() {
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
        let errors =
            resolve_src("type Alias = Missing").expect_err("unknown type name is an error");
        assert!(matches!(
            errors[0],
            BindingResolutionError::UnboundTypeName { ref name, .. } if name == "Missing"
        ));
    }

    #[test]
    fn a_builtin_resolves_to_its_prelude_binding() {
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
        let errors = resolve_src("let outer = { let inner = 1  inner }\ninner")
            .expect_err("`inner` must not escape its block");
        assert!(matches!(
            errors[0],
            BindingResolutionError::UnboundName { .. }
        ));
    }
}
