use std::collections::BTreeMap;

use miette::SourceSpan;

use crate::binding_resolution::binding_resolved::{
    ResolvedBinaryExpression, ResolvedFunctionInvocationExpression, ResolvedLetExpression,
    ResolvedTypeExpression, ResolvedTypeExpressionKind, ResolvedUnaryExpression, TypeBindingId,
};
use crate::binding_resolution::{
    BindingId, ResolvedBinding, ResolvedExpression, ResolvedExpressionKind, ResolvedLambda,
    ResolvedLiteral,
};
use crate::parse::ast::{BinaryOp, UnaryOp};
use crate::type_check::binding_table::{Scheme, TypeBindingToTypeMap};

use super::binding_table::{BindingLookup, BindingToTypeMap};
use super::error::TypeCheckError;
use super::typed_ast::{
    ExpressionKind, Lambda, Literal, Param, Type, TypeCheckedExpression, TypeCheckedLiteral,
};
use super::unification::UnificationMap;
use crate::prelude::PRELUDE_TYPES;

struct LetGeneralisationLevel(usize);
impl LetGeneralisationLevel {
    pub fn new() -> LetGeneralisationLevel {
        LetGeneralisationLevel(0)
    }
}

pub(super) struct InferenceCtx<'a> {
    pub(super) variable_env: BindingToTypeMap,
    pub(super) type_env: TypeBindingToTypeMap,
    pub(super) unification_map: UnificationMap,
    let_level: LetGeneralisationLevel,
    bindings: &'a [ResolvedBinding],
}

impl<'a> InferenceCtx<'a> {
    pub(super) fn new(bindings: &'a [ResolvedBinding], type_binding_count: usize) -> Self {
        InferenceCtx {
            bindings,
            variable_env: BindingToTypeMap::new(bindings.len()),
            type_env: create_type_env_with_prelude_types(type_binding_count),
            unification_map: UnificationMap::new(),
            let_level: LetGeneralisationLevel::new(),
        }
    }

    pub(super) fn infer(
        &mut self,
        untyped_expression: ResolvedExpression,
    ) -> Result<TypeCheckedExpression, TypeCheckError> {
        let span = untyped_expression.span;

        let (kind, ty) = match untyped_expression.kind {
            ResolvedExpressionKind::Literal(lit) => self.infer_literal(lit),
            ResolvedExpressionKind::Var(binding_id) => self.infer_var(binding_id, span),
            ResolvedExpressionKind::Binary(binary) => self.infer_binary_operation(binary, span),
            ResolvedExpressionKind::Unary(unary) => self.infer_unary_operation(unary),
            ResolvedExpressionKind::Lambda(resolved_lambda) => {
                self.infer_lambda(resolved_lambda, span)
            }
            ResolvedExpressionKind::FunctionInvocation(function_invocation) => {
                self.infer_function_invocation(function_invocation, span)
            }

            ResolvedExpressionKind::Let(let_expression) => {
                self.infer_let_expression(let_expression, span)
            }

            ResolvedExpressionKind::Block(expressions) => self.infer_block(expressions),
            ResolvedExpressionKind::If {
                condition,
                true_condition,
                false_condition,
            } => self.infer_if(condition, true_condition, false_condition, span),
            ResolvedExpressionKind::TypeDeclaration {
                identifier,
                type_expression,
            } => self.infer_type_declaration(identifier, type_expression),
            ResolvedExpressionKind::FieldAccess { target, field_name } => {
                self.infer_field_access(target, field_name, span)
            }
        }?;

        Ok(TypeCheckedExpression { kind, span, ty })
    }

    fn infer_block(
        &mut self,
        expressions: Vec<ResolvedExpression>,
    ) -> Result<(ExpressionKind, Type), TypeCheckError> {
        let analysed = expressions
            .into_iter()
            .map(|e| self.infer(e))
            .collect::<Result<Vec<_>, _>>()?;
        let ty = analysed.last().map_or(Type::Unit, |e| e.ty.clone());
        Ok((ExpressionKind::Block(analysed), ty))
    }

    fn infer_if(
        &mut self,
        condition: Box<ResolvedExpression>,
        true_condition: Box<ResolvedExpression>,
        false_condition: Option<Box<ResolvedExpression>>,
        span: SourceSpan,
    ) -> Result<(ExpressionKind, Type), TypeCheckError> {
        let typed_condition = self.infer(*condition)?;

        self.unification_map
            .unify(&typed_condition.ty, &Type::Literal(Literal::Bool), span)?;

        let true_condition = self.infer(*true_condition)?;
        let true_condition_type = true_condition.ty.clone();

        match false_condition {
            None => Ok((
                ExpressionKind::If {
                    condition: Box::new(typed_condition),
                    then_branch: Box::new(true_condition),
                    else_branch: None,
                },
                true_condition_type,
            )),
            Some(false_condition) => {
                let false_condition = self.infer(*false_condition)?;

                self.unification_map
                    .unify(&false_condition.ty, &true_condition.ty, span)?;

                Ok((
                    ExpressionKind::If {
                        condition: Box::new(typed_condition),
                        then_branch: Box::new(true_condition),
                        else_branch: Some(Box::new(false_condition)),
                    },
                    true_condition_type,
                ))
            }
        }
    }

    fn infer_type_declaration(
        &mut self,
        identifier: TypeBindingId,
        type_expression: ResolvedTypeExpression,
    ) -> Result<(ExpressionKind, Type), TypeCheckError> {
        let evaluated_type = get_type_from_type_expression(type_expression, self)?;

        self.type_env.set(identifier, evaluated_type.clone());

        Ok((
            ExpressionKind::TypeDeclaration {
                identifier,
                type_expression: evaluated_type.clone(),
            },
            evaluated_type,
        ))
    }

    fn infer_field_access(
        &mut self,
        target: Box<ResolvedExpression>,
        field_name: String,
        span: SourceSpan,
    ) -> Result<(ExpressionKind, Type), TypeCheckError> {
        let target = self.infer(*target)?;

        let Type::Record(field_types) = self.unification_map.representative(&target.ty) else {
            return Err(TypeCheckError::NotARecord {
                found: target.ty,
                span,
            });
        };

        let field_type =
            field_types
                .get(&field_name)
                .ok_or_else(|| TypeCheckError::RecordDoesNotHaveField {
                    field_name: field_name.clone(),
                    available: field_types.keys().cloned().collect(),
                    span,
                })?;

        let ty = (**field_type).clone();

        Ok((
            ExpressionKind::FieldAccess {
                target: Box::new(target),
                field_name,
            },
            ty,
        ))
    }

    fn infer_unary_operation(
        &mut self,
        ResolvedUnaryExpression(op, expr): ResolvedUnaryExpression,
    ) -> Result<(ExpressionKind, Type), TypeCheckError> {
        let operand = self.infer(*expr)?;

        let ty = match op {
            UnaryOp::Neg => Type::Literal(Literal::Int),
            UnaryOp::Not => Type::Literal(Literal::Bool),
        };

        self.unification_map.unify(&operand.ty, &ty, operand.span)?;

        Ok((ExpressionKind::Unary(op, Box::new(operand)), ty))
    }

    fn infer_binary_operation(
        &mut self,
        ResolvedBinaryExpression(op, lhs, rhs): ResolvedBinaryExpression,
        span: SourceSpan,
    ) -> Result<(ExpressionKind, Type), TypeCheckError> {
        let lhs = self.infer(*lhs)?;
        let rhs = self.infer(*rhs)?;

        match op {
            BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div => unify_binary_op(
                &mut self.unification_map,
                op,
                lhs,
                Type::Literal(Literal::Int),
                rhs,
                Type::Literal(Literal::Int),
                Type::Literal(Literal::Int),
            ),
            BinaryOp::Lt
            | BinaryOp::Gt
            | BinaryOp::Le
            | BinaryOp::Ge
            | BinaryOp::Eq
            | BinaryOp::Neq => unify_binary_op(
                &mut self.unification_map,
                op,
                lhs,
                Type::Literal(Literal::Int),
                rhs,
                Type::Literal(Literal::Int),
                Type::Literal(Literal::Bool),
            ),
            BinaryOp::And | BinaryOp::Or => unify_binary_op(
                &mut self.unification_map,
                op,
                lhs,
                Type::Literal(Literal::Bool),
                rhs,
                Type::Literal(Literal::Bool),
                Type::Literal(Literal::Bool),
            ),
            BinaryOp::Pipe => {
                let Type::Fn {
                    parameter: input,
                    return_type: output,
                } = self.unification_map.representative(&rhs.ty)
                else {
                    return Err(TypeCheckError::NotAFunction {
                        found: rhs.ty,
                        span,
                    });
                };

                let Some(input) = input else {
                    return Err(TypeCheckError::PipeIntoArgumentlessFunction { span: rhs.span });
                };

                self.unification_map.unify(&lhs.ty, &input, span)?;

                Ok((
                    ExpressionKind::Binary(op, Box::new(lhs), Box::new(rhs)),
                    *output,
                ))
            }
        }
    }

    fn infer_var(
        &mut self,
        variable_binding_id: BindingId,
        span: SourceSpan,
    ) -> Result<(ExpressionKind, Type), TypeCheckError> {
        let ty = match self.variable_env.get(variable_binding_id) {
            Some(ty) => ty.clone(),
            None => {
                return Err(TypeCheckError::MissingAnnotation {
                    name: self.bindings.lookup(variable_binding_id).name.clone(),
                    span,
                });
            }
        };

        Ok((ExpressionKind::Var(variable_binding_id), ty))
    }

    fn infer_literal(
        &mut self,
        literal: ResolvedLiteral,
    ) -> Result<(ExpressionKind, Type), TypeCheckError> {
        let value = match literal {
            ResolvedLiteral::Unit => (
                ExpressionKind::Literal(TypeCheckedLiteral::Unit),
                Type::Literal(Literal::Unit),
            ),
            ResolvedLiteral::Int(value) => (
                ExpressionKind::Literal(TypeCheckedLiteral::Int(value)),
                Type::Literal(Literal::Int),
            ),

            ResolvedLiteral::String(value) => (
                ExpressionKind::Literal(TypeCheckedLiteral::String(value)),
                Type::Literal(Literal::String),
            ),

            ResolvedLiteral::Bool(value) => (
                ExpressionKind::Literal(TypeCheckedLiteral::Bool(value)),
                Type::Literal(Literal::Bool),
            ),

            ResolvedLiteral::Float(value) => (
                ExpressionKind::Literal(TypeCheckedLiteral::Float(value)),
                Type::Literal(Literal::Float),
            ),

            ResolvedLiteral::Record(record) => {
                let fields: BTreeMap<String, TypeCheckedExpression> = record
                    .into_iter()
                    .map(|(key, value)| {
                        let value = self.infer(value)?;
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
        };

        Ok(value)
    }

    fn infer_lambda(
        &mut self,
        resolved_lambda: ResolvedLambda,
        span: SourceSpan,
    ) -> Result<(ExpressionKind, Type), TypeCheckError> {
        let ResolvedLambda {
            parameter,
            body,
            return_type,
        } = resolved_lambda;

        let parameter: Option<Param> = parameter
            .map(|untyped_param| {
                let type_from_type_dec = resolve_type_dec(untyped_param.type_dec, self)?;

                self.variable_env
                    .set(untyped_param.binding, type_from_type_dec.clone());

                Ok(Param {
                    binding: untyped_param.binding,
                    ty: type_from_type_dec,
                })
            })
            .transpose()?;
        let param_type = parameter.as_ref().map(|p| Box::new(p.ty.clone()));

        let body = self.infer(*body)?;
        let return_type = resolve_type_dec(return_type, self)?;
        self.unification_map.unify(&body.ty, &return_type, span)?;

        let lambda_return_type = body.ty.clone();

        Ok((
            ExpressionKind::Lambda(Lambda {
                parameter,
                body: Box::new(body),
            }),
            Type::Fn {
                parameter: param_type,
                return_type: Box::new(lambda_return_type),
            },
        ))
    }

    fn infer_function_invocation(
        &mut self,
        ResolvedFunctionInvocationExpression {
            arguments,
            function,
        }: ResolvedFunctionInvocationExpression,
        span: SourceSpan,
    ) -> Result<(ExpressionKind, Type), TypeCheckError> {
        let analysed_args = arguments
            .into_iter()
            .map(|arg| self.infer(arg))
            .collect::<Result<Vec<_>, _>>()?;

        let typed_function = self.infer(*function)?;

        let output_type = get_type_after_applying_arguments(
            &mut self.unification_map,
            &typed_function.ty,
            &analysed_args,
            span,
        )?;

        Ok((
            ExpressionKind::FunctionInvocation {
                function: Box::new(typed_function),
                arguments: analysed_args,
            },
            output_type,
        ))
    }

    fn infer_let_expression(
        &mut self,
        ResolvedLetExpression {
            binding,
            type_dec,
            value,
        }: ResolvedLetExpression,
        span: SourceSpan,
    ) -> Result<(ExpressionKind, Type), TypeCheckError> {
        let inferred_type = self.infer(*value)?;

        let bound_type_from_declaration = resolve_type_dec(type_dec, self)?;

        // We try to unify the inferred type with the bound type.
        self.unification_map
            .unify(&inferred_type.ty, &bound_type_from_declaration, span)?;

        // let scheme = generalise(&bound_type_from_declaration, ctx, unification_map);
        self.variable_env
            .set(binding, bound_type_from_declaration.clone());

        Ok((
            ExpressionKind::Let {
                binding,
                value: Box::new(inferred_type),
            },
            Type::Unit,
        ))
    }
}

fn create_type_env_with_prelude_types(
    type_binding_count: usize,
) -> super::binding_table::GenericTypeMap<TypeBindingId, Type> {
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

// fn generalise(
//     bound_type_from_declaration: &Type,
//     ctx: &mut InferenceCtx,
//     unification_map: &mut UnificationMap,
// ) -> Scheme {
//     // We need to find any free type variables recorded in the type
//     // and record them in the scheme to generalise them.
//     let root_type = unification_map.subsitute(bound_type_from_declaration);

//     // Collect type variables that are not
// }

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
        ResolvedTypeExpressionKind::Lambda(resolved_lambda_type_expression) => Ok(Type::Fn {
            parameter: resolved_lambda_type_expression
                .parameter
                .map(|param| get_type_from_type_expression(*param.type_dec, ctx))
                .transpose()?
                .map(Box::new),
            return_type: Box::new(get_type_from_type_expression(
                *resolved_lambda_type_expression.return_type,
                ctx,
            )?),
        }),
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
        if let Type::Fn {
            parameter: None,
            return_type,
        } = resolved
        {
            return Ok(*return_type);
        }
        return Ok(resolved);
    }

    match &resolved {
        Type::Fn { .. } | Type::Var(_) => {
            apply_arguments(unification_map, &resolved, arguments, span)
        }
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
        Type::Fn {
            parameter: Some(param_type),
            return_type,
        } => {
            unification_map.unify(&param_type, &arg.ty, span)?;
            apply_arguments(unification_map, &return_type, &arguments[1..], span)
        }

        Type::Fn {
            parameter: None,
            return_type: _,
        } => Err(TypeCheckError::ArgumentsToArgumentlessFunction { span }),

        callee @ Type::Var(_) => {
            let result = Type::Var(unification_map.mint_new_type_var());
            let fn_shape = Type::Fn {
                parameter: Some(Box::new(arg.ty.clone())),
                return_type: Box::new(result.clone()),
            };
            unification_map.unify(&callee, &fn_shape, span)?;
            apply_arguments(unification_map, &result, &arguments[1..], span)
        }

        _ => Err(TypeCheckError::TooManyArguments { span }),
    }
}

fn resolve_type_dec(
    dec: Option<ResolvedTypeExpression>,
    ctx: &mut InferenceCtx,
) -> Result<Type, TypeCheckError> {
    match dec {
        Some(dec) => {
            let evaluated_type = get_type_from_type_expression(dec, ctx)?;

            Ok(evaluated_type)
        }
        None => Ok(Type::Var(ctx.unification_map.mint_new_type_var())),
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
        let fn_type = Type::Fn {
            parameter: None,
            return_type: Box::new(Type::Unit),
        };
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
        let fn_type = Type::Fn {
            parameter: Some(Box::new(Type::Literal(Literal::Int))),
            return_type: Box::new(Type::Literal(Literal::Int)),
        };
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
