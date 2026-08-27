use std::collections::BTreeMap;

use miette::SourceSpan;

use crate::binding_resolution::binding_resolved::{
    ResolvedTypeExpression, ResolvedTypeExpressionKind, TypeBindingId,
};
use crate::binding_resolution::{
    ResolvedBinding, ResolvedExpression, ResolvedExpressionKind, ResolvedLambda, ResolvedLiteral,
};
use crate::parse::ast::{BinaryOp, UnaryOp};
use crate::type_check::binding_table::TypeBindingToTypeMap;

use super::binding_table::{BindingLookup, BindingToTypeMap};
use super::error::TypeCheckError;
use super::typed_ast::{
    ExpressionKind, Lambda, Literal, Param, Type, TypeCheckedExpression, TypeCheckedLiteral,
};
use super::unification::UnificationMap;
use crate::prelude::PRELUDE_TYPES;

pub(super) struct InferenceCtx {
    pub(super) variable_env: BindingToTypeMap,
    pub(super) type_env: TypeBindingToTypeMap,
}

impl InferenceCtx {
    pub(super) fn new(binding_count: usize, type_binding_count: usize) -> InferenceCtx {
        let type_env = create_type_env_with_prelude_types(type_binding_count);

        InferenceCtx {
            variable_env: BindingToTypeMap::new(binding_count),
            type_env,
        }
    }
}

fn create_type_env_with_prelude_types(
    type_binding_count: usize,
) -> super::binding_table::GenericTypeMap<TypeBindingId> {
    debug_assert!(
        type_binding_count >= PRELUDE_TYPES.len(),
        "resolve seeds the prelude before any user declaration"
    );

    let type_env = PRELUDE_TYPES.iter().enumerate().fold(
        TypeBindingToTypeMap::new(type_binding_count),
        |mut env, (index, prelude_type)| {
            env.set(TypeBindingId(index), prelude_type.ty.clone());
            env
        },
    );
    type_env
}

pub(super) fn infer_type_of_expression(
    untyped_expression: ResolvedExpression,
    ctx: &mut InferenceCtx,
    unification_map: &mut UnificationMap,
    bindings: &[ResolvedBinding],
) -> Result<TypeCheckedExpression, TypeCheckError> {
    let span = untyped_expression.span;
    let (kind, ty) = match untyped_expression.kind {
        ResolvedExpressionKind::Literal(ResolvedLiteral::Unit) => (
            ExpressionKind::Literal(TypeCheckedLiteral::Unit),
            Type::Literal(Literal::Unit),
        ),
        ResolvedExpressionKind::Literal(ResolvedLiteral::Int(value)) => (
            ExpressionKind::Literal(TypeCheckedLiteral::Int(value)),
            Type::Literal(Literal::Int),
        ),

        ResolvedExpressionKind::Literal(ResolvedLiteral::String(value)) => (
            ExpressionKind::Literal(TypeCheckedLiteral::String(value)),
            Type::Literal(Literal::String),
        ),

        ResolvedExpressionKind::Literal(ResolvedLiteral::Bool(value)) => (
            ExpressionKind::Literal(TypeCheckedLiteral::Bool(value)),
            Type::Literal(Literal::Bool),
        ),

        ResolvedExpressionKind::Literal(ResolvedLiteral::Float(value)) => (
            ExpressionKind::Literal(TypeCheckedLiteral::Float(value)),
            Type::Literal(Literal::Float),
        ),

        ResolvedExpressionKind::Literal(ResolvedLiteral::Record(record)) => {
            let fields = record
                .into_iter()
                .map(|(key, value)| {
                    let value = infer_type_of_expression(value, ctx, unification_map, bindings)?;
                    Ok((key, value))
                })
                .collect::<Result<BTreeMap<_, _>, TypeCheckError>>()?;

            let ty = Type::Record(
                fields
                    .iter()
                    .map(|(key, field)| (key.clone(), Box::new(field.ty.clone())))
                    .collect(),
            );

            (
                ExpressionKind::Literal(TypeCheckedLiteral::Record(fields)),
                ty,
            )
        }
        ResolvedExpressionKind::Var(binding_id) => {
            let ty = match ctx.variable_env.get(binding_id) {
                Some(ty) => ty.clone(),
                None => {
                    return Err(TypeCheckError::MissingAnnotation {
                        name: bindings.lookup(binding_id).name.clone(),
                        span,
                    });
                }
            };
            (ExpressionKind::Var(binding_id), ty)
        }

        ResolvedExpressionKind::Binary(op, lhs, rhs) => {
            let lhs = infer_type_of_expression(*lhs, ctx, unification_map, bindings)?;
            let rhs = infer_type_of_expression(*rhs, ctx, unification_map, bindings)?;

            match op {
                BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div => unify_binary_op(
                    unification_map,
                    op,
                    lhs,
                    Type::Literal(Literal::Int),
                    rhs,
                    Type::Literal(Literal::Int),
                    Type::Literal(Literal::Int),
                )?,
                BinaryOp::Lt
                | BinaryOp::Gt
                | BinaryOp::Le
                | BinaryOp::Ge
                | BinaryOp::Eq
                | BinaryOp::Neq => unify_binary_op(
                    unification_map,
                    op,
                    lhs,
                    Type::Literal(Literal::Int),
                    rhs,
                    Type::Literal(Literal::Int),
                    Type::Literal(Literal::Bool),
                )?,
                BinaryOp::And | BinaryOp::Or => unify_binary_op(
                    unification_map,
                    op,
                    lhs,
                    Type::Literal(Literal::Bool),
                    rhs,
                    Type::Literal(Literal::Bool),
                    Type::Literal(Literal::Bool),
                )?,
                BinaryOp::Pipe => {
                    let Type::Fn(input, output) = unification_map.representative(&rhs.ty) else {
                        return Err(TypeCheckError::NotAFunction {
                            found: rhs.ty,
                            span,
                        });
                    };

                    let Some(input) = input else {
                        return Err(TypeCheckError::PipeIntoArgumentlessFunction {
                            span: rhs.span,
                        });
                    };

                    unification_map.unify(&lhs.ty, &input, span)?;

                    (
                        ExpressionKind::Binary(op, Box::new(lhs), Box::new(rhs)),
                        *output,
                    )
                }
            }
        }

        ResolvedExpressionKind::Unary(op, operand) => {
            let operand = infer_type_of_expression(*operand, ctx, unification_map, bindings)?;

            let ty = match op {
                UnaryOp::Neg => Type::Literal(Literal::Int),
                UnaryOp::Not => Type::Literal(Literal::Bool),
            };
            unification_map.unify(&operand.ty, &ty, operand.span)?;
            (ExpressionKind::Unary(op, Box::new(operand)), ty)
        }

        ResolvedExpressionKind::Lambda(resolved_lambda) => {
            let ResolvedLambda {
                parameter,
                body,
                return_type,
            } = resolved_lambda;

            let parameter: Option<Param> = parameter
                .map(|untyped_param| {
                    let type_from_type_dec =
                        resolve_type_dec(untyped_param.type_dec, unification_map, ctx)?;
                    ctx.variable_env
                        .set(untyped_param.binding, type_from_type_dec.clone());
                    Ok(Param {
                        binding: untyped_param.binding,
                        ty: type_from_type_dec,
                    })
                })
                .transpose()?;
            let param_type = parameter.as_ref().map(|p| Box::new(p.ty.clone()));

            let body = infer_type_of_expression(*body, ctx, unification_map, bindings)?;
            let return_type = resolve_type_dec(return_type, unification_map, ctx)?;
            unification_map.unify(&body.ty, &return_type, span)?;
            let lambda_return_type = body.ty.clone();

            (
                ExpressionKind::Lambda(Lambda {
                    parameter,
                    body: Box::new(body),
                }),
                Type::Fn(param_type, Box::new(lambda_return_type)),
            )
        }

        ResolvedExpressionKind::FunctionInvocation {
            function,
            arguments,
        } => {
            let analysed_args = arguments
                .into_iter()
                .map(|arg| infer_type_of_expression(arg, ctx, unification_map, bindings))
                .collect::<Result<Vec<_>, _>>()?;

            let typed_function =
                infer_type_of_expression(*function, ctx, unification_map, bindings)?;

            let output_type = get_type_after_applying_arguments(
                unification_map,
                &typed_function.ty,
                &analysed_args,
                span,
            )?;

            (
                ExpressionKind::FunctionInvocation {
                    function: Box::new(typed_function),
                    arguments: analysed_args,
                },
                output_type,
            )
        }

        ResolvedExpressionKind::Let {
            binding,
            type_dec,
            value,
        } => {
            let value = infer_type_of_expression(*value, ctx, unification_map, bindings)?;

            let bound_ty = resolve_type_dec(type_dec, unification_map, ctx)?;

            ctx.variable_env.set(binding, bound_ty.clone());

            unification_map.unify(&value.ty, &bound_ty, span)?;

            (
                ExpressionKind::Let {
                    binding,
                    value: Box::new(value),
                },
                Type::Unit,
            )
        }

        ResolvedExpressionKind::Block(expressions) => {
            let analysed = expressions
                .into_iter()
                .map(|e| infer_type_of_expression(e, ctx, unification_map, bindings))
                .collect::<Result<Vec<_>, _>>()?;
            let ty = analysed.last().map_or(Type::Unit, |e| e.ty.clone());
            (ExpressionKind::Block(analysed), ty)
        }
        ResolvedExpressionKind::If {
            condition,
            true_condition,
            false_condition,
        } => {
            let typed_condition =
                infer_type_of_expression(*condition, ctx, unification_map, bindings)?;

            unification_map.unify(&typed_condition.ty, &Type::Literal(Literal::Bool), span)?;

            let true_condition =
                infer_type_of_expression(*true_condition, ctx, unification_map, bindings)?;
            let true_condition_type = true_condition.ty.clone();

            match false_condition {
                None => (
                    ExpressionKind::If {
                        condition: Box::new(typed_condition),
                        then_branch: Box::new(true_condition),
                        else_branch: None,
                    },
                    true_condition_type,
                ),
                Some(false_condition) => {
                    let false_condition =
                        infer_type_of_expression(*false_condition, ctx, unification_map, bindings)?;

                    unification_map.unify(&false_condition.ty, &true_condition.ty, span)?;

                    (
                        ExpressionKind::If {
                            condition: Box::new(typed_condition),
                            then_branch: Box::new(true_condition),
                            else_branch: Some(Box::new(false_condition)),
                        },
                        true_condition_type,
                    )
                }
            }
        }

        ResolvedExpressionKind::TypeDeclaration {
            identifier,
            type_expression,
        } => {
            let evaluated_type = get_type_from_type_expression(type_expression, ctx)?;

            ctx.type_env.set(identifier, evaluated_type.clone());

            (
                ExpressionKind::TypeDeclaration {
                    identifier,
                    type_expression: evaluated_type.clone(),
                },
                evaluated_type,
            )
        }
        ResolvedExpressionKind::FieldAccess { target, field_name } => {
            let target = infer_type_of_expression(*target, ctx, unification_map, bindings)?;

            let Type::Record(field_types) = unification_map.representative(&target.ty) else {
                return Err(TypeCheckError::NotARecord {
                    found: target.ty,
                    span,
                });
            };

            let field_type = field_types.get(&field_name).ok_or_else(|| {
                TypeCheckError::RecordDoesNotHaveField {
                    field_name: field_name.clone(),
                    available: field_types.keys().cloned().collect(),
                    span,
                }
            })?;

            let ty = (**field_type).clone();

            (
                ExpressionKind::FieldAccess {
                    target: Box::new(target),
                    field_name,
                },
                ty,
            )
        }
    };

    Ok(TypeCheckedExpression { kind, span, ty })
}

fn get_type_from_type_expression(
    type_expression: ResolvedTypeExpression,
    ctx: &mut InferenceCtx,
) -> Result<Type, TypeCheckError> {
    match type_expression.kind {
        ResolvedTypeExpressionKind::Named(type_binding_id) => {
            match ctx.type_env.get(type_binding_id) {
                None => Err(TypeCheckError::InternalError {
                    message: String::from("Could not find the type for the given type binding id"),
                    span: type_expression.span,
                }),
                Some(referenced_type) => Ok(referenced_type.clone()),
            }
        }
        ResolvedTypeExpressionKind::Record(btree_map) => Ok(Type::Record(
            btree_map
                .into_iter()
                .map(|(key, value)| Ok((key, Box::new(get_type_from_type_expression(value, ctx)?))))
                .collect::<Result<_, TypeCheckError>>()?,
        )),
    }
}

fn unify_binary_op(
    unification_map: &mut UnificationMap,
    op: BinaryOp,
    lhs: TypeCheckedExpression,
    lhs_type: Type,
    rhs: TypeCheckedExpression,
    rhs_type: Type,
    return_type: Type,
) -> Result<(ExpressionKind, Type), TypeCheckError> {
    unification_map.unify(&lhs.ty, &lhs_type, lhs.span)?;
    unification_map.unify(&rhs.ty, &rhs_type, rhs.span)?;

    Ok((
        ExpressionKind::Binary(op, Box::new(lhs), Box::new(rhs)),
        return_type,
    ))
}

pub(super) fn get_type_after_applying_arguments(
    unification_map: &mut UnificationMap,
    fn_type: &Type,
    arguments: &[TypeCheckedExpression],
    span: SourceSpan,
) -> Result<Type, TypeCheckError> {
    let resolved = unification_map.representative(fn_type);

    if arguments.is_empty() {
        if let Type::Fn(None, return_type) = resolved {
            return Ok(*return_type);
        }
        return Ok(resolved);
    }

    match &resolved {
        Type::Fn(..) | Type::Var(_) => apply_arguments(unification_map, &resolved, arguments, span),
        _ => Err(TypeCheckError::NotAFunction {
            found: resolved,
            span,
        }),
    }
}

fn apply_arguments(
    unification_map: &mut UnificationMap,
    fn_type: &Type,
    arguments: &[TypeCheckedExpression],
    span: SourceSpan,
) -> Result<Type, TypeCheckError> {
    let Some(arg) = arguments.first() else {
        return Ok(fn_type.clone());
    };

    match unification_map.representative(fn_type) {
        Type::Fn(Some(param_type), return_type) => {
            unification_map.unify(&param_type, &arg.ty, span)?;
            apply_arguments(unification_map, &return_type, &arguments[1..], span)
        }

        Type::Fn(None, _) => Err(TypeCheckError::ArgumentsToArgumentlessFunction { span }),

        callee @ Type::Var(_) => {
            let result = Type::Var(unification_map.mint_new_type_var());
            let fn_shape = Type::Fn(Some(Box::new(arg.ty.clone())), Box::new(result.clone()));
            unification_map.unify(&callee, &fn_shape, span)?;
            apply_arguments(unification_map, &result, &arguments[1..], span)
        }

        _ => Err(TypeCheckError::TooManyArguments { span }),
    }
}

fn resolve_type_dec(
    dec: Option<ResolvedTypeExpression>,
    unification_map: &mut UnificationMap,
    ctx: &mut InferenceCtx,
) -> Result<Type, TypeCheckError> {
    match dec {
        Some(dec) => {
            let evaluated_type = get_type_from_type_expression(dec, ctx)?;

            Ok(evaluated_type)
        }
        None => Ok(Type::Var(unification_map.mint_new_type_var())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::type_check::typed_ast::Literal;

    fn int_arg() -> TypeCheckedExpression {
        TypeCheckedExpression {
            kind: ExpressionKind::Literal(TypeCheckedLiteral::Int(0)),
            span: SourceSpan::from((0, 0)),
            ty: Type::Literal(Literal::Int),
        }
    }

    #[test]
    fn arguments_to_argumentless_function_is_an_error() {
        let fn_type = Type::Fn(None, Box::new(Type::Unit));
        let err = get_type_after_applying_arguments(
            &mut UnificationMap::new(),
            &fn_type,
            &[int_arg()],
            SourceSpan::from((0, 0)),
        )
        .expect_err("applying an argument to a nullary function is an error");
        assert!(matches!(
            err,
            TypeCheckError::ArgumentsToArgumentlessFunction { .. }
        ));
    }

    #[test]
    fn applying_correct_arguments_returns_result_type() {
        let fn_type = Type::Fn(
            Some(Box::new(Type::Literal(Literal::Int))),
            Box::new(Type::Literal(Literal::Int)),
        );
        let result = get_type_after_applying_arguments(
            &mut UnificationMap::new(),
            &fn_type,
            &[int_arg()],
            SourceSpan::from((0, 0)),
        )
        .expect("applying a matching argument should succeed");
        assert_eq!(result, Type::Literal(Literal::Int));
    }
}
