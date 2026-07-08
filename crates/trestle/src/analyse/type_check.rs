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

use std::env::join_paths;

use miette::SourceSpan;

use crate::analyse::analysed::{
    AnalysedExpression, BindingId, ExpressionKind, Lambda, Literal, Param, Type,
};
use crate::analyse::resolved::{ResolvedBinding, ResolvedExpression, ResolvedExpressionKind};
use crate::parse::ast::TypeDeclaration;

use super::AnalysisError;
use super::analysed::AnalysedProgram;
use super::resolved::ResolvedProgram;

struct Env {
    types: Vec<Option<Type>>,
}
impl Env {
    fn new(binding_count: usize) -> Env {
        Env {
            types: Vec::with_capacity(binding_count),
        }
    }

    fn set(&mut self, id: BindingId, ty: Type) {
        self.types[id.0] = Some(ty);
    }

    fn get(&self, id: BindingId) -> Option<&Type> {
        self.types[id.0].as_ref()
    }
}

/// Type-check a name-resolved program into a fully typed [`AnalysedProgram`].
pub(super) fn type_check(program: &ResolvedProgram) -> Result<AnalysedProgram, Vec<AnalysisError>> {
    // Goal: Infer the typing information for all bindings
    let mut env = Env::new(program.bindings.len());

    let mut expressions = Vec::with_capacity(program.expressions.len());
    let mut errors = Vec::new();

    for untyped_expression in &program.expressions {
        match infer_type_of_expression(untyped_expression, &mut env, &program.bindings) {
            Ok(expression) => expressions.push(expression),
            Err(error) => errors.push(error),
        }
    }

    match errors.is_empty() {
        true => Ok(AnalysedProgram {
            expressions,
            bindings: todo!("build BindingInfo table from env + program.bindings"),
        }),
        false => Err(errors),
    }
}

struct ResolvedBindings([ResolvedBinding]);
impl ResolvedBindings {
    pub fn get(&self, binding: BindingId) -> String {
        self.0[binding.0].name.clone()
    }
}

fn infer_type_of_expression(
    untyped_expression: &ResolvedExpression,
    env: &mut Env,
    bindings: &ResolvedBindings,
) -> Result<AnalysedExpression, AnalysisError> {
    // Borrow the kind: boxed operands aren't `Copy`, so matching by value would move them.
    match &untyped_expression.kind {
        ResolvedExpressionKind::Int(value) => Ok(AnalysedExpression {
            kind: ExpressionKind::Int(*value),
            span: untyped_expression.span,
            // An integer literal is a `Literal::Int`.
            ty: Type::Literal(Literal::Int),
        }),

        ResolvedExpressionKind::Var(binding_id) => {
            // The binding's type was recorded when its `let`/lambda-param was analysed.
            // If none is known at the use site, the binding needs an annotation.
            let ty = match env.get(*binding_id) {
                Some(ty) => ty.clone(),
                None => {
                    return Err(AnalysisError::MissingAnnotation {
                        name: bindings.get(*binding_id),
                        span: untyped_expression.span,
                    });
                }
            };
            Ok(AnalysedExpression {
                kind: ExpressionKind::Var(*binding_id),
                span: untyped_expression.span,
                ty,
            })
        }

        ResolvedExpressionKind::Add(lhs, rhs) => {
            let lhs = infer_type_of_expression(lhs, env, bindings)?;
            let rhs = infer_type_of_expression(rhs, env, bindings)?;
            // Both operands must be `Int`; unifying each against `Int` yields the result type.
            let int = Type::Literal(Literal::Int);
            unify(&lhs.ty, &int, lhs.span)?;
            unify(&rhs.ty, &int, rhs.span)?;
            Ok(AnalysedExpression {
                kind: ExpressionKind::Add(Box::new(lhs), Box::new(rhs)),
                span: untyped_expression.span,
                ty: int,
            })
        }

        ResolvedExpressionKind::Mul(lhs, rhs) => {
            let lhs = infer_type_of_expression(lhs, env, bindings)?;
            let rhs = infer_type_of_expression(rhs, env, bindings)?;
            // Both operands must be `Int`; unifying each against `Int` yields the result type.
            let int = Type::Literal(Literal::Int);
            unify(&lhs.ty, &int, lhs.span)?;
            unify(&rhs.ty, &int, rhs.span)?;
            Ok(AnalysedExpression {
                kind: ExpressionKind::Mul(Box::new(lhs), Box::new(rhs)),
                span: untyped_expression.span,
                ty: int,
            })
        }

        ResolvedExpressionKind::Lambda(resolved_lambda) => {
            // Resolve the parameter's annotation into a `Type`, record it in `env` (via
            // `env.set`) so the body can see it, and build the analysed `Param`.
            let parameter: Option<Param> = resolved_lambda
                .parameter
                .as_ref()
                .map(|untyped_param| {
                    let type_from_type_dec = resolve_type_dec(&untyped_param.type_dec)?;
                    Ok(Param {
                        binding: untyped_param.binding,
                        ty: type_from_type_dec,
                    })
                })
                .transpose()?;

            // Infer the body under the (now parameter-extended) environment.
            let body = infer_type_of_expression(&resolved_lambda.body, env, bindings)?;
            let unified_return_type = resolved_lambda
                .return_type
                .as_ref()
                .map(|specified_type| {
                    resolve_type_dec(&specified_type).and_then(|found_type| {
                        unify(&body.ty, &found_type, untyped_expression.span)
                    })
                })
                .transpose()?
                .unwrap_or_else(|| body.ty.clone());

            Ok(AnalysedExpression {
                kind: ExpressionKind::Lambda(Lambda {
                    parameter,
                    body: Box::new(body),
                }),
                span: untyped_expression.span,
                // The lambda's type is `Fn(param.ty, body.ty)`.
                ty: unified_return_type,
            })
        }

        ResolvedExpressionKind::FunctionInvocation(binding_id, args) => {
            let analysed_args = args
                .iter()
                .map(|arg| infer_type_of_expression(arg, env, bindings))
                .collect::<Result<Vec<_>, _>>()?;

            // We now need to check that we can apply the types of the arguments to the parameters of the fuction.
            // First, we resolve the type from the binding_id
            let function_type = env.get(*binding_id);

            if let None = function_type {
                return Err(AnalysisError::UnboundName {
                    name: bindings.get(*binding_id),
                    span: untyped_expression.span,
                });
            }

            // Unify the function's type with the argument types.

            // Fold the args through the callee's curried `Fn(a, Fn(b, r))` type in `env`,
            // peeling one arrow (and checking one arg) per element; the leftover is the result.
            Ok(AnalysedExpression {
                kind: ExpressionKind::FunctionInvocation(*binding_id, analysed_args),
                span: untyped_expression.span,
                ty: todo!(),
            })
        }

        ResolvedExpressionKind::Let { binding, value } => {
            let value = infer_type_of_expression(value, env, bindings)?;
            // Record the binding's type for later references: `env.set(*binding, value.ty…)`.
            Ok(AnalysedExpression {
                kind: ExpressionKind::Let {
                    binding: *binding,
                    value: Box::new(value),
                },
                span: untyped_expression.span,
                ty: todo!(),
            })
        }
    }
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

/// Interpret a raw type annotation into a concrete [`Type`]
/// (`"Int"`/`"Bool"`/`"String"` → [`Literal`](super::analysed::Literal); unknown → an error).
fn resolve_type_dec(dec: &TypeDeclaration) -> Result<Type, AnalysisError> {
    todo!()
}
