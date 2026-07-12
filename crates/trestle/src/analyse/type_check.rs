//! Pass 2 — type checking. Turns a [`ResolvedProgram`] into an [`AnalysedProgram`] by computing
//! a [`Type`](super::analysed::Type) for every node and interpreting annotations.
//!
//! Intended implementation:
//! - A bidirectional walk `check_expr(expr, expected: Option<&Type>) -> analysed::Expression`:
//!   *synthesise* a node's type bottom-up where its form determines it (literals, annotated
//!   params, applications of known functions), and *check* against `expected` where a type flows
//!   in from context (an annotated `let`, or a call site). This is what lets an unannotated
//!   parameter be typed when — and only when — it sits in a checking position.
//! - Interpret [`ast::TypeDeclaration::Named`](crate::parse::ast::TypeDeclaration) into a
//!   concrete [`Type`] (`"Int"`/`"Bool"`/`"String"` → [`Literal`](super::analysed::Literal);
//!   unknown → an error). This is where type-*name* resolution will grow when generics arrive.
//! - Fold a `FunctionInvocation`'s argument `Vec` left-to-right through the callee's curried
//!   `Fn(a, Fn(b, r))` type, peeling one arrow (and checking one argument) per element.
//! - Build the analysed [`BindingInfo`](super::analysed::BindingInfo) table by pairing each
//!   [`ResolvedBinding`](super::resolved::ResolvedBinding)'s name+span with its computed type.
//! - Report mismatches as [`AnalysisError::TypeMismatch`]; collect rather than bail.

use miette::SourceSpan;

use crate::analyse::analysed::{
    AnalysedBinding, AnalysedExpression, AnalysedLiteral, BindingId, ExpressionKind, Lambda,
    Literal, Param, Type,
};
use crate::analyse::resolved::{
    ResolvedBinding, ResolvedExpression, ResolvedExpressionKind, ResolvedLambda, ResolvedLiteral,
};
use crate::parse::ast::{BinaryOp, TypeDeclaration};

use super::AnalysisError;
use super::analysed::AnalysedProgram;
use super::resolved::ResolvedProgram;

struct TypeEnv {
    types: Vec<Option<Type>>,
}
impl TypeEnv {
    fn new(binding_count: usize) -> TypeEnv {
        TypeEnv {
            types: vec![None; binding_count],
        }
    }

    fn set(&mut self, id: BindingId, ty: Type) {
        self.types[id.0] = Some(ty);
    }

    fn get(&self, id: BindingId) -> Option<&Type> {
        self.types[id.0].as_ref()
    }
}

trait BindingLookup {
    fn lookup(&self, id: BindingId) -> &ResolvedBinding;
}
impl BindingLookup for [ResolvedBinding] {
    fn lookup(&self, id: BindingId) -> &ResolvedBinding {
        &self[id.0]
    }
}

/// Pair each binding with the type computed for it during the walk, **moving** its name across.
/// A binding still untyped afterwards is an [`UntypedBindingAfterTypeCheck`] error. Consumes the
/// binding table since it's the last reader of it.
///
/// [`UntypedBindingAfterTypeCheck`]: AnalysisError::UntypedBindingAfterTypeCheck
fn zip_bindings_with_types(
    bindings: Vec<ResolvedBinding>,
    env: &TypeEnv,
) -> Result<Vec<AnalysedBinding>, AnalysisError> {
    assert_eq!(bindings.len(), env.types.len());

    bindings
        .into_iter()
        .enumerate()
        .map(|(index, binding)| match env.get(BindingId(index)) {
            Some(ty) => Ok(AnalysedBinding {
                name: binding.name,
                ty: ty.clone(),
                span: binding.span,
            }),
            None => Err(AnalysisError::UntypedBindingAfterTypeCheck {
                name: binding.name,
                span: binding.span,
            }),
        })
        .collect()
}

struct TypeCheckState {
    type_env: TypeEnv,
    expressions: Vec<AnalysedExpression>,
    errors: Vec<AnalysisError>,
}
/// Type-check a name-resolved program into a fully typed [`AnalysedProgram`].
pub(super) fn type_check(program: ResolvedProgram) -> Result<AnalysedProgram, Vec<AnalysisError>> {
    let ResolvedProgram {
        expressions,
        bindings,
    } = program;

    // Borrow `bindings` for id lookups during the walk; it's consumed afterwards (moving each
    // name across) to build the typed table.
    let expression_count = expressions.len();
    let final_state = expressions.into_iter().fold(
        TypeCheckState {
            expressions: Vec::with_capacity(expression_count),
            errors: Vec::new(),
            type_env: TypeEnv::new(bindings.len()),
        },
        |mut state, untyped_expression| {
            match infer_type_of_expression(untyped_expression, &mut state.type_env, &bindings) {
                Ok(expression) => state.expressions.push(expression),
                Err(error) => state.errors.push(error),
            }

            state
        },
    );

    let typed_bindings =
        zip_bindings_with_types(bindings, &final_state.type_env).map_err(|err| vec![err])?;

    match final_state.errors.is_empty() {
        true => Ok(AnalysedProgram {
            expressions: final_state.expressions,
            bindings: typed_bindings,
        }),
        false => Err(final_state.errors),
    }
}

fn infer_type_of_expression(
    untyped_expression: ResolvedExpression,
    env: &mut TypeEnv,
    bindings: &[ResolvedBinding],
) -> Result<AnalysedExpression, AnalysisError> {
    let span = untyped_expression.span;
    let (kind, ty) = match untyped_expression.kind {
        ResolvedExpressionKind::Literal(ResolvedLiteral::Int(value)) => (
            ExpressionKind::Literal(AnalysedLiteral::Int(value)),
            // An integer literal is a `Literal::Int`.
            Type::Literal(Literal::Int),
        ),

        ResolvedExpressionKind::Literal(ResolvedLiteral::String(value)) => (
            // The name-resolved string moves straight through — no copy.
            ExpressionKind::Literal(AnalysedLiteral::String(value)),
            Type::Literal(Literal::String),
        ),

        ResolvedExpressionKind::Literal(ResolvedLiteral::Bool(value)) => (
            ExpressionKind::Literal(AnalysedLiteral::Bool(value)),
            Type::Literal(Literal::Bool),
        ),

        ResolvedExpressionKind::Literal(ResolvedLiteral::Float(value)) => (
            ExpressionKind::Literal(AnalysedLiteral::Float(value)),
            Type::Literal(Literal::Float),
        ),

        ResolvedExpressionKind::Var(binding_id) => {
            // The binding's type was recorded when its `let`/lambda-param was analysed.
            // If none is known at the use site, the binding needs an annotation.
            let ty = match env.get(binding_id) {
                Some(ty) => ty.clone(),
                None => {
                    return Err(AnalysisError::MissingAnnotation {
                        name: bindings.lookup(binding_id).name.clone(),
                        span,
                    });
                }
            };
            (ExpressionKind::Var(binding_id), ty)
        }

        ResolvedExpressionKind::Binary(op, lhs, rhs) => {
            let lhs = infer_type_of_expression(*lhs, env, bindings)?;
            let rhs = infer_type_of_expression(*rhs, env, bindings)?;
            // Every operator (arithmetic and comparison) takes two `Int`s for now; unify each
            // operand against `Int`. The result type is what distinguishes them: arithmetic
            // yields an `Int`, comparison yields a `Bool`.
            let int = Type::Literal(Literal::Int);
            unify(&lhs.ty, &int, lhs.span)?;
            unify(&rhs.ty, &int, rhs.span)?;
            let result_ty = match op {
                BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div => int,
                BinaryOp::Lt
                | BinaryOp::Gt
                | BinaryOp::Le
                | BinaryOp::Ge
                | BinaryOp::Eq
                | BinaryOp::Neq => Type::Literal(Literal::Bool),
            };
            (
                ExpressionKind::Binary(op, Box::new(lhs), Box::new(rhs)),
                result_ty,
            )
        }

        ResolvedExpressionKind::Lambda(resolved_lambda) => {
            let ResolvedLambda {
                parameter,
                body,
                return_type,
            } = resolved_lambda;

            // Resolve the parameter's annotation into a `Type`, record it in `env` (via
            // `env.set`) so the body can see it, and build the analysed `Param`.
            let parameter: Option<Param> = parameter
                .map(|untyped_param| {
                    let type_from_type_dec = resolve_type_dec(&untyped_param.type_dec, span)?;
                    env.set(untyped_param.binding, type_from_type_dec.clone());
                    Ok(Param {
                        binding: untyped_param.binding,
                        ty: type_from_type_dec,
                    })
                })
                .transpose()?;
            let param_type = parameter.as_ref().map(|p| Box::new(p.ty.clone()));

            // Infer the body under the (now parameter-extended) environment.
            let body = infer_type_of_expression(*body, env, bindings)?;
            let unified_return_type = return_type
                .map(|specified_type| {
                    resolve_type_dec(&specified_type, span)
                        .and_then(|found_type| unify(&body.ty, &found_type, span))
                })
                .transpose()?
                .unwrap_or_else(|| body.ty.clone());

            (
                ExpressionKind::Lambda(Lambda {
                    parameter,
                    body: Box::new(body),
                }),
                // The lambda's type is `Fn(param.ty, body.ty)`.
                Type::Fn(param_type, Box::new(unified_return_type)),
            )
        }

        ResolvedExpressionKind::FunctionInvocation(binding_id, args) => {
            let analysed_args = args
                .into_iter()
                .map(|arg| infer_type_of_expression(arg, env, bindings))
                .collect::<Result<Vec<_>, _>>()?;

            // We now need to check that we can apply the types of the arguments to the parameters of the fuction.
            // First, we resolve the type from the binding_id
            let Some(function_type) = env.get(binding_id) else {
                return Err(AnalysisError::UnboundName {
                    name: bindings.lookup(binding_id).name.clone(),
                    span,
                });
            };

            let output_type =
                get_type_after_applying_arguments(function_type, &analysed_args, span)?;

            // Fold the args through the callee's curried `Fn(a, Fn(b, r))` type in `env`,
            // peeling one arrow (and checking one arg) per element; the leftover is the result.
            (
                ExpressionKind::FunctionInvocation(binding_id, analysed_args),
                output_type,
            )
        }

        ResolvedExpressionKind::Let { binding, value } => {
            let value = infer_type_of_expression(*value, env, bindings)?;
            // Record the binding's type for later references: `env.set(binding, value.ty…)`.
            env.set(binding, value.ty.clone());

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
                .map(|e| infer_type_of_expression(e, env, bindings))
                .collect::<Result<Vec<_>, _>>()?;
            let ty = analysed.last().map_or(Type::Unit, |e| e.ty.clone());
            (ExpressionKind::Block(analysed), ty)
        }
        ResolvedExpressionKind::If {
            condition,
            true_condition,
            false_condition,
        } => {
            let typed_condition = infer_type_of_expression(*condition, env, bindings)?;
            // Unify the typed_condition value with boolean.
            unify(&typed_condition.ty, &Type::Literal(Literal::Bool), span)?;

            let true_condition = infer_type_of_expression(*true_condition, env, bindings)?;
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
                        infer_type_of_expression(*false_condition, env, bindings)?;

                    unify(&false_condition.ty, &true_condition.ty, span)?;

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

    Ok(AnalysedExpression { kind, span, ty })
}

/// Reconcile two types, returning the type they agree on or a [`TypeMismatch`] at `span`.
///
/// [`TypeMismatch`]: AnalysisError::TypeMismatch
///
/// While every [`Type`] is concrete this is just an equality check that hands the type back.
/// When `Type` grows a `Var` variant for inference, this is the seam that becomes real
/// unification — solving variables and threading a substitution — without any call site
/// changing: they already consume the returned type rather than assuming one.
fn unify(found: &Type, expected: &Type, span: SourceSpan) -> Result<Type, AnalysisError> {
    if found == expected {
        Ok(found.clone())
    } else {
        Err(AnalysisError::TypeMismatch {
            expected: expected.clone(),
            found: found.clone(),
            span,
        })
    }
}

fn get_type_after_applying_arguments(
    fn_type: &Type,
    arguments: &[AnalysedExpression],
    span: SourceSpan,
) -> Result<Type, AnalysisError> {
    // Before applying anything: a non-function callee is `NotAFunction`. This is distinct from
    // the over-application error, which we can only detect once we've started peeling arrows.
    if !arguments.is_empty() && !matches!(fn_type, Type::Fn(..)) {
        return Err(AnalysisError::NotAFunction {
            found: fn_type.clone(),
            span,
        });
    }
    apply_arguments(fn_type, arguments, span)
}

/// Fold `arguments` through the callee's curried `Fn(a, Fn(b, r))` type, peeling (and checking)
/// one argument per arrow. The caller (`get_type_after_applying_arguments`) has already ensured
/// that a non-function `fn_type` here means the caller over-applied.
fn apply_arguments(
    fn_type: &Type,
    arguments: &[AnalysedExpression],
    span: SourceSpan,
) -> Result<Type, AnalysisError> {
    // No arguments left — the remaining type is the call's result (a fully-applied function's
    // return type, or the function itself for a bare/partial reference).
    let Some(arg) = arguments.first() else {
        return Ok(fn_type.clone());
    };

    // Still have an argument but no arrow to peel: the callee was a function (guaranteed by the
    // entry check), so this is over-application.
    let Type::Fn(param_type, return_type) = fn_type else {
        return Err(AnalysisError::TooManyArguments { span });
    };

    // The function is nullary but was handed an argument.
    let Some(param_type) = param_type else {
        return Err(AnalysisError::ArgumentsToArgumentlessFunction { span });
    };

    // If the types unify, we can move on to the rest of the arguments.
    unify(param_type, &arg.ty, span)?;
    apply_arguments(return_type, &arguments[1..], span)
}

/// Interpret a raw type annotation into a concrete [`Type`]
/// (`"Int"`/`"Bool"`/`"String"` → [`Literal`](super::analysed::Literal); unknown → an error).
fn resolve_type_dec(dec: &TypeDeclaration, span: SourceSpan) -> Result<Type, AnalysisError> {
    let TypeDeclaration::Named(name) = dec;
    match name.as_str() {
        "Int" => Ok(Type::Literal(Literal::Int)),
        "Bool" => Ok(Type::Literal(Literal::Bool)),
        "Float" => Ok(Type::Literal(Literal::Float)),
        "String" => Ok(Type::Literal(Literal::String)),
        _ => Err(AnalysisError::UnknownType {
            name: name.clone(),
            span,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn analyse_src(src: &str) -> Result<AnalysedProgram, Vec<AnalysisError>> {
        let program = crate::parse::parse(src).expect("test source should parse");
        crate::analyse::analyse(program)
    }

    /// A dummy `Int` literal argument for driving `get_type_after_applying_arguments` directly.
    fn int_arg() -> AnalysedExpression {
        AnalysedExpression {
            kind: ExpressionKind::Literal(AnalysedLiteral::Int(0)),
            span: SourceSpan::from((0, 0)),
            ty: Type::Literal(Literal::Int),
        }
    }

    #[test]
    fn too_many_arguments_is_an_error() {
        // `f` takes one argument; applying two over-applies it.
        let errors = analyse_src("let f = (a: Int) => a\nf(1, 2)")
            .expect_err("over-application is an error");
        assert!(matches!(errors[0], AnalysisError::TooManyArguments { .. }));
    }

    #[test]
    fn arguments_to_argumentless_function_is_an_error() {
        // Nullary functions can't be written in source yet (the grammar requires a parameter),
        // so drive the checker directly with a `Fn(None, _)` type given one argument.
        let fn_type = Type::Fn(None, Box::new(Type::Unit));
        let err =
            get_type_after_applying_arguments(&fn_type, &[int_arg()], SourceSpan::from((0, 0)))
                .expect_err("applying an argument to a nullary function is an error");
        assert!(matches!(
            err,
            AnalysisError::ArgumentsToArgumentlessFunction { .. }
        ));
    }

    #[test]
    fn applying_correct_arguments_returns_result_type() {
        // `Fn(Int, Int)` applied to one argument yields its result type.
        let fn_type = Type::Fn(
            Some(Box::new(Type::Literal(Literal::Int))),
            Box::new(Type::Literal(Literal::Int)),
        );
        let result =
            get_type_after_applying_arguments(&fn_type, &[int_arg()], SourceSpan::from((0, 0)))
                .expect("applying a matching argument should succeed");
        assert_eq!(result, Type::Literal(Literal::Int));
    }
}
