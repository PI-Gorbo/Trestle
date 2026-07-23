//! Bottom-up type inference over the binding-resolved tree: synthesise a [`Type`] for every node,
//! unifying operands, lambda bodies, `let` values, and call arguments against what their context
//! requires.

use miette::SourceSpan;

use crate::binding_resolution::{
    ResolvedBinding, ResolvedExpression, ResolvedExpressionKind, ResolvedLambda, ResolvedLiteral,
};
use crate::parse::ast::{BinaryOp, TypeDeclaration, UnaryOp};

use super::binding_table::{BindingLookup, BindingToTypeMap};
use super::error::TypeCheckError;
use super::typed_ast::{
    ExpressionKind, Lambda, Literal, Param, Type, TypeCheckedExpression, TypeCheckedLiteral,
};
use super::unification::{UnificationMap, unify};

pub(super) fn infer_type_of_expression(
    untyped_expression: ResolvedExpression,
    env: &mut BindingToTypeMap,
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
            // An integer literal is a `Literal::Int`.
            Type::Literal(Literal::Int),
        ),

        ResolvedExpressionKind::Literal(ResolvedLiteral::String(value)) => (
            // The name-resolved string moves straight through — no copy.
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

        ResolvedExpressionKind::Var(binding_id) => {
            // The binding's type was recorded when its `let`/lambda-param was analysed.
            // If none is known at the use site, the binding needs an annotation.
            let ty = match env.get(binding_id) {
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
            let lhs = infer_type_of_expression(*lhs, env, unification_map, bindings)?;
            let rhs = infer_type_of_expression(*rhs, env, unification_map, bindings)?;
            // The operator fixes both the operand type and the result type. Arithmetic is
            // `Int × Int → Int`; comparison is `Int × Int → Bool`; the boolean combinators are
            // `Bool × Bool → Bool`. Unify each operand against the required operand type.
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
                    // The right-hand side is the function being piped into. A bare reference to a
                    // let-bound function has a type *variable* as its type, so resolve it to its
                    // representative before requiring an `Fn` shape.
                    let Type::Fn(input, output) = unification_map.representative(&rhs.ty) else {
                        return Err(TypeCheckError::NotAFunction {
                            found: rhs.ty,
                            span,
                        });
                    };

                    let Some(input) = input else {
                        return Err(TypeCheckError::PipeIntoArgumentlessFunction { span: rhs.span });
                    };

                    unify(unification_map, &lhs.ty, &input, span)?;

                    (
                        ExpressionKind::Binary(op, Box::new(lhs), Box::new(rhs)),
                        *output,
                    )
                }
            }
        }

        ResolvedExpressionKind::Unary(op, operand) => {
            let operand = infer_type_of_expression(*operand, env, unification_map, bindings)?;
            // `-` negates an `Int` (→ `Int`); `!` inverts a `Bool` (→ `Bool`). Operand and result
            // type coincide for both, so unify the operand against that one type.
            let ty = match op {
                UnaryOp::Neg => Type::Literal(Literal::Int),
                UnaryOp::Not => Type::Literal(Literal::Bool),
            };
            unify(unification_map, &operand.ty, &ty, operand.span)?;
            (ExpressionKind::Unary(op, Box::new(operand)), ty)
        }

        ResolvedExpressionKind::Lambda(resolved_lambda) => {
            let ResolvedLambda {
                parameter,
                body,
                return_type,
            } = resolved_lambda;

            // Resolve the parameter's annotation into a `Type`, record it in `env` (via
            // `env.set`) so the body can see it, and build the typed `Param`.
            let parameter: Option<Param> = parameter
                .map(|untyped_param| {
                    let type_from_type_dec =
                        resolve_type_dec(unification_map, &untyped_param.type_dec, span)?;
                    env.set(untyped_param.binding, type_from_type_dec.clone());
                    Ok(Param {
                        binding: untyped_param.binding,
                        ty: type_from_type_dec,
                    })
                })
                .transpose()?;
            let param_type = parameter.as_ref().map(|p| Box::new(p.ty.clone()));

            // Infer the body under the (now parameter-extended) environment.
            let body = infer_type_of_expression(*body, env, unification_map, bindings)?;
            let return_type = resolve_type_dec(unification_map, &return_type, span)?;
            unify(unification_map, &body.ty, &return_type, span)?;
            let lambda_return_type = body.ty.clone();

            (
                ExpressionKind::Lambda(Lambda {
                    parameter,
                    body: Box::new(body),
                }),
                // The lambda's type is `Fn(param.ty, body.ty)`.
                Type::Fn(param_type, Box::new(lambda_return_type)),
            )
        }

        ResolvedExpressionKind::FunctionInvocation(binding_id, args) => {
            let analysed_args = args
                .into_iter()
                .map(|arg| infer_type_of_expression(arg, env, unification_map, bindings))
                .collect::<Result<Vec<_>, _>>()?;

            // We now need to check that we can apply the types of the arguments to the parameters of
            // the function. First, we resolve the type from the binding_id.
            let Some(function_type) = env.get(binding_id) else {
                return Err(TypeCheckError::MissingAnnotation {
                    name: bindings.lookup(binding_id).name.clone(),
                    span,
                });
            };

            let output_type = get_type_after_applying_arguments(
                unification_map,
                function_type,
                &analysed_args,
                span,
            )?;

            // Fold the args through the callee's curried `Fn(a, Fn(b, r))` type in `env`,
            // peeling one arrow (and checking one arg) per element; the leftover is the result.
            (
                ExpressionKind::FunctionInvocation(binding_id, analysed_args),
                output_type,
            )
        }

        ResolvedExpressionKind::Let {
            binding,
            type_dec,
            value,
        } => {
            let value = infer_type_of_expression(*value, env, unification_map, bindings)?;
            // With an annotation the binding takes the annotated type; without one it takes the
            // value's inferred type. Record it *before* unifying so that a mismatch still leaves the
            // binding typed — otherwise `zip_bindings_with_types` would mask the `TypeMismatch` with
            // an `UntypedBindingAfterTypeCheck`.
            let bound_ty = resolve_type_dec(unification_map, &type_dec, span)?;

            env.set(binding, bound_ty.clone());
            // The value's type must unify with the binding's; for an annotated `let` a differing
            // value type is a `TypeMismatch` (expected = annotation, found = value).
            unify(unification_map, &value.ty, &bound_ty, span)?;

            (
                ExpressionKind::Let {
                    binding,
                    value: Box::new(value),
                },
                Type::Unit,
            )
        }

        // A block's value is its last expression's; earlier ones are typed for effect. The
        // grammar guarantees at least one element, but fall back to `Unit` defensively.
        ResolvedExpressionKind::Block(expressions) => {
            let analysed = expressions
                .into_iter()
                .map(|e| infer_type_of_expression(e, env, unification_map, bindings))
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
                infer_type_of_expression(*condition, env, unification_map, bindings)?;
            // Unify the typed_condition value with boolean.
            unify(
                unification_map,
                &typed_condition.ty,
                &Type::Literal(Literal::Bool),
                span,
            )?;

            let true_condition =
                infer_type_of_expression(*true_condition, env, unification_map, bindings)?;
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
                        infer_type_of_expression(*false_condition, env, unification_map, bindings)?;

                    unify(
                        unification_map,
                        &false_condition.ty,
                        &true_condition.ty,
                        span,
                    )?;

                    (
                        ExpressionKind::If {
                            condition: Box::new(typed_condition),
                            then_branch: Box::new(true_condition),
                            else_branch: None,
                        },
                        true_condition_type,
                    )
                }
            }
        }
    };

    Ok(TypeCheckedExpression { kind, span, ty })
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
    unify(unification_map, &lhs.ty, &lhs_type, lhs.span)?;
    unify(unification_map, &rhs.ty, &rhs_type, rhs.span)?;

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
    // Resolve the callee to what it currently stands for: a let-bound function's binding type is
    // a type variable, so `fn_type` here is often a `Var` whose root is a concrete `Fn`.
    let resolved = unification_map.representative(fn_type);

    if arguments.is_empty() {
        // A zero-argument call `f()` is a nullary invocation: peel the single `Fn(None, R)`
        // arrow to its result `R`. A bare/partial reference just yields the callee itself.
        if let Type::Fn(None, return_type) = resolved {
            return Ok(*return_type);
        }
        return Ok(resolved);
    }

    // Before applying anything: a *concrete* non-function callee is `NotAFunction`. A `Var` callee
    // is a not-yet-known function and is constrained during peeling; over-application is a distinct
    // error we can only detect once we've started peeling a concrete function's arrows.
    match &resolved {
        Type::Fn(..) | Type::Var(_) => apply_arguments(unification_map, &resolved, arguments, span),
        _ => Err(TypeCheckError::NotAFunction {
            found: resolved,
            span,
        }),
    }
}

/// Fold `arguments` through the callee's curried `Fn(a, Fn(b, r))` type, peeling (and checking)
/// one argument per arrow. A still-unknown callee (a type variable) is constrained to
/// `Fn(arg, fresh_result)` and application continues through the fresh result.
fn apply_arguments(
    unification_map: &mut UnificationMap,
    fn_type: &Type,
    arguments: &[TypeCheckedExpression],
    span: SourceSpan,
) -> Result<Type, TypeCheckError> {
    // No arguments left — the remaining type is the call's result (a fully-applied function's
    // return type, or the function itself for a bare/partial reference).
    let Some(arg) = arguments.first() else {
        return Ok(fn_type.clone());
    };

    match unification_map.representative(fn_type) {
        // Peel one arrow: the argument must unify with the parameter.
        Type::Fn(Some(param_type), return_type) => {
            unify(unification_map, &param_type, &arg.ty, span)?;
            apply_arguments(unification_map, &return_type, &arguments[1..], span)
        }

        // The function is nullary but was handed an argument.
        Type::Fn(None, _) => Err(TypeCheckError::ArgumentsToArgumentlessFunction { span }),

        // The callee is a not-yet-known function: constrain its variable to `Fn(arg, result)`
        // and continue with the fresh result variable.
        callee @ Type::Var(_) => {
            let result = Type::Var(unification_map.mint_new_type_var());
            let fn_shape = Type::Fn(Some(Box::new(arg.ty.clone())), Box::new(result.clone()));
            unify(unification_map, &callee, &fn_shape, span)?;
            apply_arguments(unification_map, &result, &arguments[1..], span)
        }

        // Started from a function (guaranteed by the entry check) but ran out of arrows to peel:
        // the caller over-applied.
        _ => Err(TypeCheckError::TooManyArguments { span }),
    }
}

/// Interpret a raw type annotation into a concrete [`Type`]
/// (`"Int"`/`"Bool"`/`"String"` → [`Literal`]; unknown → an error).
fn resolve_type_dec(
    unification_map: &mut UnificationMap,
    dec: &Option<TypeDeclaration>,
    span: SourceSpan,
) -> Result<Type, TypeCheckError> {
    match dec {
        Some(dec) => {
            let TypeDeclaration::Named(name) = dec;
            match name.as_str() {
                "Int" => Ok(Type::Literal(Literal::Int)),
                "Bool" => Ok(Type::Literal(Literal::Bool)),
                "Float" => Ok(Type::Literal(Literal::Float)),
                "String" => Ok(Type::Literal(Literal::String)),
                _ => Err(TypeCheckError::UnknownType {
                    name: name.clone(),
                    span,
                }),
            }
        }
        None => Ok(Type::Var(unification_map.mint_new_type_var())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::type_check::typed_ast::Literal;

    /// A dummy `Int` literal argument for driving `get_type_after_applying_arguments` directly.
    fn int_arg() -> TypeCheckedExpression {
        TypeCheckedExpression {
            kind: ExpressionKind::Literal(TypeCheckedLiteral::Int(0)),
            span: SourceSpan::from((0, 0)),
            ty: Type::Literal(Literal::Int),
        }
    }

    #[test]
    fn arguments_to_argumentless_function_is_an_error() {
        // Nullary functions can't be written in source yet (the grammar requires a parameter),
        // so drive the checker directly with a `Fn(None, _)` type given one argument.
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
        // `Fn(Int, Int)` applied to one argument yields its result type.
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
