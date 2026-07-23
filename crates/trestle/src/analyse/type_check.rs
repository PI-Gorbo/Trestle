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
    Literal, Param, Type, TypeVarId,
};
use crate::analyse::resolved::{
    ResolvedBinding, ResolvedExpression, ResolvedExpressionKind, ResolvedLambda, ResolvedLiteral,
};
use crate::parse::ast::{BinaryOp, TypeDeclaration, UnaryOp};

use super::AnalysisError;
use super::analysed::AnalysedProgram;
use super::resolved::ResolvedProgram;

struct BindingToTypeMap {
    types: Vec<Option<Type>>,
}
impl BindingToTypeMap {
    fn new(binding_count: usize) -> BindingToTypeMap {
        BindingToTypeMap {
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
    binding_type_map: &BindingToTypeMap,
) -> Result<Vec<AnalysedBinding>, AnalysisError> {
    assert_eq!(bindings.len(), binding_type_map.types.len());

    bindings
        .into_iter()
        .enumerate()
        .map(
            |(index, binding)| match binding_type_map.get(BindingId(index)) {
                Some(ty) => Ok(AnalysedBinding {
                    name: binding.name,
                    ty: ty.clone(),
                    span: binding.span,
                }),
                None => Err(AnalysisError::UntypedBindingAfterTypeCheck {
                    name: binding.name,
                    span: binding.span,
                }),
            },
        )
        .collect()
}

enum UnionNode {
    Reference(TypeVarId),
    RootUnionNode(RootUnionNode),
}

enum RootUnionNode {
    FreeTypeVariable,
    Concrete(Type),
}

struct UnificationMap {
    map: Vec<UnionNode>,
}

struct FreeTypeVariableNotFoundError {
    type_variable_id: TypeVarId,
}

struct TypeMismatch {
    expected: Type,
    found: Type,
}

enum UnifyError {
    TypeMismatch(TypeMismatch),
    FreeTypeVariableNotFoundError(FreeTypeVariableNotFoundError),
}

impl UnificationMap {
    pub fn new() -> UnificationMap {
        UnificationMap { map: Vec::new() }
    }

    pub fn find_root(&self, var_id: TypeVarId) -> Option<(TypeVarId, &RootUnionNode)> {
        self.map.get(var_id.0).and_then(|found| match found {
            UnionNode::Reference(type_var_id) => self.find_root(*type_var_id),
            UnionNode::RootUnionNode(root) => Some((var_id, root)),
        })
    }

    pub fn subsitute(&self, ty: &Type) -> Type {
        match ty {
            Type::Unit => ty.clone(),
            Type::Literal(_) => ty.clone(),
            Type::Var(type_var_id) => match self.find_root(*type_var_id) {
                // A concrete root may itself hold more variables (e.g. `Fn`), so substitute it.
                Some((_, RootUnionNode::Concrete(concrete))) => self.subsitute(concrete),
                // A still-free variable collapses to its canonical root.
                Some((root_id, RootUnionNode::FreeTypeVariable)) => Type::Var(root_id),
                None => ty.clone(),
            },
            Type::Fn(param, result) => Type::Fn(
                param.as_ref().map(|param| Box::new(self.subsitute(param))),
                Box::new(self.subsitute(result)),
            ),
        }
    }

    /// Follow a type variable to its current representative: the concrete type it's been unified
    /// with, or a canonical `Var(root)` if still free. Non-variables pass through. Shallow — it
    /// does not descend into `Fn` children (application peels/unifies those one level at a time).
    pub fn representative(&self, ty: &Type) -> Type {
        match ty {
            Type::Var(id) => match self.find_root(*id) {
                Some((_, RootUnionNode::Concrete(concrete))) => concrete.clone(),
                Some((root_id, RootUnionNode::FreeTypeVariable)) => Type::Var(root_id),
                None => ty.clone(),
            },
            _ => ty.clone(),
        }
    }

    pub fn mint_new_type_var(&mut self) -> TypeVarId {
        let new_type_var = TypeVarId(self.map.len());
        self.map
            .push(UnionNode::RootUnionNode(RootUnionNode::FreeTypeVariable));

        new_type_var
    }

    pub fn set(
        &mut self,
        type_var_id: TypeVarId,
        node: UnionNode,
    ) -> Result<(), FreeTypeVariableNotFoundError> {
        match self.map.get_mut(type_var_id.0) {
            Some(slot) => {
                *slot = node;
                Ok(())
            }
            None => Err(FreeTypeVariableNotFoundError {
                type_variable_id: type_var_id,
            }),
        }
    }

    pub fn union_with_concrete_type(
        &mut self,
        variable: TypeVarId,
        concrete_type: &Type,
    ) -> Result<(), UnifyError> {
        let (root_id, root_node) =
            self.find_root(variable)
                .ok_or(UnifyError::FreeTypeVariableNotFoundError(
                    FreeTypeVariableNotFoundError {
                        type_variable_id: variable,
                    },
                ))?;

        match root_node {
            RootUnionNode::Concrete(root_node_concrete_type) => {
                match *concrete_type == *root_node_concrete_type {
                    true => Ok(()),
                    false => Err(UnifyError::TypeMismatch(TypeMismatch {
                        expected: root_node_concrete_type.clone(),
                        found: concrete_type.clone(),
                    })),
                }
            }
            RootUnionNode::FreeTypeVariable => {
                self.set(
                    root_id,
                    UnionNode::RootUnionNode(RootUnionNode::Concrete(concrete_type.clone())),
                )
                .map_err(UnifyError::FreeTypeVariableNotFoundError)?;

                Ok(())
            }
        }
    }

    pub fn union_vars(
        &mut self,
        first_var_id: TypeVarId,
        second_var_id: TypeVarId,
    ) -> Result<(), UnifyError> {
        let (first_found_root_id, first_found) =
            self.find_root(first_var_id)
                .ok_or(UnifyError::FreeTypeVariableNotFoundError(
                    FreeTypeVariableNotFoundError {
                        type_variable_id: first_var_id,
                    },
                ))?;

        let (second_found_root_id, second_found) =
            self.find_root(second_var_id)
                .ok_or(UnifyError::FreeTypeVariableNotFoundError(
                    FreeTypeVariableNotFoundError {
                        type_variable_id: second_var_id,
                    },
                ))?;

        match (first_found, second_found) {
            (RootUnionNode::FreeTypeVariable, RootUnionNode::FreeTypeVariable) => {
                self.set(
                    second_found_root_id,
                    UnionNode::Reference(first_found_root_id),
                )
                .map_err(UnifyError::FreeTypeVariableNotFoundError)?;

                Ok(())
            }
            (RootUnionNode::FreeTypeVariable, RootUnionNode::Concrete(_)) => {
                // Update the free type variable to be a reference to the concrete type.
                self.set(
                    first_found_root_id,
                    UnionNode::Reference(second_found_root_id),
                )
                .map_err(UnifyError::FreeTypeVariableNotFoundError)?;
                Ok(())
            }
            (RootUnionNode::Concrete(_), RootUnionNode::FreeTypeVariable) => {
                // Update the free type variable to be a reference to the concrete type.
                self.set(
                    second_found_root_id,
                    UnionNode::Reference(first_found_root_id),
                )
                .map_err(UnifyError::FreeTypeVariableNotFoundError)?;
                Ok(())
            }
            (
                RootUnionNode::Concrete(first_concrete_type_var),
                RootUnionNode::Concrete(second_concrete_type_var),
            ) => match first_concrete_type_var == second_concrete_type_var {
                true => Ok(()),
                false => Err(UnifyError::TypeMismatch(TypeMismatch {
                    expected: first_concrete_type_var.clone(),
                    found: second_concrete_type_var.clone(),
                })),
            },
        }
    }
}

fn subsitute(map: &UnificationMap, expr: &mut AnalysedExpression) {
    // The tree shape is unchanged — only the `ty` fields get rewritten — so walk the boxed/vec
    // children by `&mut` (deref coercion turns `&mut Box<_>` into `&mut AnalysedExpression`)
    // and reuse every existing allocation.
    match &mut expr.kind {
        ExpressionKind::Literal(_) => {}
        ExpressionKind::Var(_) => {}
        ExpressionKind::Binary(_, lhs, rhs) => {
            subsitute(map, lhs);
            subsitute(map, rhs);
        }
        ExpressionKind::Unary(_, operand) => {
            subsitute(map, operand);
        }
        ExpressionKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            subsitute(map, condition);
            subsitute(map, then_branch);
            if let Some(else_branch) = else_branch {
                subsitute(map, else_branch);
            }
        }
        ExpressionKind::Lambda(lambda) => {
            if let Some(param) = &mut lambda.parameter {
                param.ty = map.subsitute(&param.ty);
            }
            subsitute(map, &mut lambda.body);
        }
        ExpressionKind::FunctionInvocation(_, analysed_expressions) => {
            for arg in analysed_expressions {
                subsitute(map, arg);
            }
        }
        ExpressionKind::Let { value, .. } => {
            subsitute(map, value);
        }
        ExpressionKind::Block(analysed_expressions) => {
            for e in analysed_expressions {
                subsitute(map, e);
            }
        }
    }

    expr.ty = map.subsitute(&expr.ty);
}

struct TypeCheckState {
    binding_type_map: BindingToTypeMap,
    unification_map: UnificationMap,
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
            binding_type_map: BindingToTypeMap::new(bindings.len()),
            unification_map: UnificationMap::new(),
        },
        |mut state, untyped_expression| {
            match infer_type_of_expression(
                untyped_expression,
                &mut state.binding_type_map,
                &mut state.unification_map,
                &bindings,
            ) {
                Ok(expression) => state.expressions.push(expression),
                Err(error) => state.errors.push(error),
            }

            state
        },
    );

    let mut typed_bindings = zip_bindings_with_types(bindings, &final_state.binding_type_map)
        .map_err(|err| vec![err])?;

    // Binding types are recorded during inference with their type variables intact (a `let`
    // without an annotation is bound to a fresh `Var`), so resolve them the same way the
    // expression tree is resolved below.
    for binding in &mut typed_bindings {
        binding.ty = final_state.unification_map.subsitute(&binding.ty);
    }

    let mut subsituted_expressions = final_state.expressions;
    for expr in &mut subsituted_expressions {
        subsitute(&final_state.unification_map, expr);
    }

    match final_state.errors.is_empty() {
        true => Ok(AnalysedProgram {
            expressions: subsituted_expressions,
            bindings: typed_bindings,
        }),
        false => Err(final_state.errors),
    }
}

fn infer_type_of_expression(
    untyped_expression: ResolvedExpression,
    env: &mut BindingToTypeMap,
    unification_map: &mut UnificationMap,
    bindings: &[ResolvedBinding],
) -> Result<AnalysedExpression, AnalysisError> {
    let span = untyped_expression.span;
    let (kind, ty) = match untyped_expression.kind {
        ResolvedExpressionKind::Literal(ResolvedLiteral::Unit) => (
            ExpressionKind::Literal(AnalysedLiteral::Unit),
            // An integer literal is a `Literal::Int`.
            Type::Literal(Literal::Unit),
        ),
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
                        return Err(AnalysisError::NotAFunction {
                            found: rhs.ty,
                            span,
                        });
                    };

                    let Some(input) = input else {
                        return Err(AnalysisError::PipeIntoArgumentlessFunction { span: rhs.span });
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
            // `env.set`) so the body can see it, and build the analysed `Param`.
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

            // We now need to check that we can apply the types of the arguments to the parameters of the fuction.
            // First, we resolve the type from the binding_id
            let Some(function_type) = env.get(binding_id) else {
                return Err(AnalysisError::UnboundName {
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

    Ok(AnalysedExpression { kind, span, ty })
}

fn unify_binary_op(
    unification_map: &mut UnificationMap,
    op: BinaryOp,
    lhs: AnalysedExpression,
    lhs_type: Type,
    rhs: AnalysedExpression,
    rhs_type: Type,
    return_type: Type,
) -> Result<(ExpressionKind, Type), AnalysisError> {
    unify(unification_map, &lhs.ty, &lhs_type, lhs.span)?;
    unify(unification_map, &rhs.ty, &rhs_type, rhs.span)?;

    Ok((
        ExpressionKind::Binary(op, Box::new(lhs), Box::new(rhs)),
        return_type,
    ))
}

/// Reconcile two types, returning the type they agree on or a [`TypeMismatch`] at `span`.
///
/// [`TypeMismatch`]: AnalysisError::TypeMismatch
///
/// While every [`Type`] is concrete this is just an equality check that hands the type back.
/// When `Type` grows a `Var` variant for inference, this is the seam that becomes real
/// unification — solving variables and threading a substitution — without any call site
/// changing: they already consume the returned type rather than assuming one.
fn unify(
    unification_map: &mut UnificationMap,
    found: &Type,
    expected: &Type,
    span: SourceSpan,
) -> Result<(), AnalysisError> {
    match (expected, found) {
        (Type::Var(expected_type_var_id), Type::Var(found_type_var_id)) => unification_map
            .union_vars(*expected_type_var_id, *found_type_var_id)
            .map_err(|union_err| match union_err {
                UnifyError::TypeMismatch(type_mismatch) => AnalysisError::TypeMismatch {
                    expected: type_mismatch.expected,
                    found: type_mismatch.found,
                    span,
                },
                UnifyError::FreeTypeVariableNotFoundError(err) => AnalysisError::InternalError {
                    message: format!(
                        "type variable {} was referenced during unification but never minted",
                        err.type_variable_id.0
                    ),
                    span,
                },
            }),
        (Type::Unit, Type::Unit) => Ok(()),
        (Type::Literal(first_literal), Type::Literal(second_literal)) => {
            match first_literal == second_literal {
                true => Ok(()),
                false => create_type_mismatch_error(found, expected, span),
            }
        }
        (Type::Fn(param1, result1), Type::Fn(param2, result2)) => {
            let parma_unification = match (param1, param2) {
                (None, Some(_)) | (Some(_), None) => {
                    Err(AnalysisError::FunctionParameterMismatch {
                        expected: expected.clone(),
                        found: found.clone(),
                        span,
                    })
                }

                (None, None) => Ok(()),
                (Some(param1), Some(param2)) => unify(unification_map, param1, param2, span),
            };

            parma_unification?;
            unify(unification_map, result1, result2, span)
        }

        (Type::Literal(_), Type::Var(type_var_id)) => unification_map
            .union_with_concrete_type(*type_var_id, expected)
            .map_err(|union_err| match union_err {
                UnifyError::TypeMismatch(type_mismatch) => AnalysisError::TypeMismatch {
                    expected: type_mismatch.expected,
                    found: type_mismatch.found,
                    span,
                },
                UnifyError::FreeTypeVariableNotFoundError(err) => AnalysisError::InternalError {
                    message: format!(
                        "type variable {} was referenced during unification but never minted",
                        err.type_variable_id.0
                    ),
                    span,
                },
            }),
        (Type::Var(type_var_id), Type::Literal(_)) => unification_map
            .union_with_concrete_type(*type_var_id, found)
            .map_err(|union_err| match union_err {
                UnifyError::TypeMismatch(type_mismatch) => AnalysisError::TypeMismatch {
                    expected: type_mismatch.expected,
                    found: type_mismatch.found,
                    span,
                },
                UnifyError::FreeTypeVariableNotFoundError(err) => AnalysisError::InternalError {
                    message: format!(
                        "type variable {} was referenced during unification but never minted",
                        err.type_variable_id.0
                    ),
                    span,
                },
            }),
        (Type::Var(type_var_id), Type::Fn(_, _)) => unification_map
            .union_with_concrete_type(*type_var_id, found)
            .map_err(|union_err| match union_err {
                UnifyError::TypeMismatch(type_mismatch) => AnalysisError::TypeMismatch {
                    expected: type_mismatch.expected,
                    found: type_mismatch.found,
                    span,
                },
                UnifyError::FreeTypeVariableNotFoundError(err) => AnalysisError::InternalError {
                    message: format!(
                        "type variable {} was referenced during unification but never minted",
                        err.type_variable_id.0
                    ),
                    span,
                },
            }),
        (Type::Var(type_var_id), Type::Unit) => unification_map
            .union_with_concrete_type(*type_var_id, found)
            .map_err(|union_err| match union_err {
                UnifyError::TypeMismatch(type_mismatch) => AnalysisError::TypeMismatch {
                    expected: type_mismatch.expected,
                    found: type_mismatch.found,
                    span,
                },
                UnifyError::FreeTypeVariableNotFoundError(err) => AnalysisError::InternalError {
                    message: format!(
                        "type variable {} was referenced during unification but never minted",
                        err.type_variable_id.0
                    ),
                    span,
                },
            }),
        (Type::Fn(_, _), Type::Var(type_var_id)) => unification_map
            .union_with_concrete_type(*type_var_id, expected)
            .map_err(|union_err| match union_err {
                UnifyError::TypeMismatch(type_mismatch) => AnalysisError::TypeMismatch {
                    expected: type_mismatch.expected,
                    found: type_mismatch.found,
                    span,
                },
                UnifyError::FreeTypeVariableNotFoundError(err) => AnalysisError::InternalError {
                    message: format!(
                        "type variable {} was referenced during unification but never minted",
                        err.type_variable_id.0
                    ),
                    span,
                },
            }),

        (Type::Literal(_), Type::Unit) => create_type_mismatch_error(found, expected, span),
        (Type::Literal(_), Type::Fn(_, _)) => create_type_mismatch_error(found, expected, span),
        (Type::Fn(_, _), Type::Unit) => create_type_mismatch_error(found, expected, span),
        (Type::Fn(_, _), Type::Literal(_)) => create_type_mismatch_error(found, expected, span),
        (Type::Unit, Type::Literal(_)) => create_type_mismatch_error(found, expected, span),
        (Type::Unit, Type::Var(_)) => create_type_mismatch_error(found, expected, span),
        (Type::Unit, Type::Fn(_, _)) => create_type_mismatch_error(found, expected, span),
    }
}

fn create_type_mismatch_error(
    found: &Type,
    expected: &Type,
    span: SourceSpan,
) -> Result<(), AnalysisError> {
    Err(AnalysisError::TypeMismatch {
        expected: expected.clone(),
        found: found.clone(),
        span,
    })
}

fn get_type_after_applying_arguments(
    unification_map: &mut UnificationMap,
    fn_type: &Type,
    arguments: &[AnalysedExpression],
    span: SourceSpan,
) -> Result<Type, AnalysisError> {
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
        _ => Err(AnalysisError::NotAFunction {
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
    arguments: &[AnalysedExpression],
    span: SourceSpan,
) -> Result<Type, AnalysisError> {
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
        Type::Fn(None, _) => Err(AnalysisError::ArgumentsToArgumentlessFunction { span }),

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
        _ => Err(AnalysisError::TooManyArguments { span }),
    }
}

/// Interpret a raw type annotation into a concrete [`Type`]
/// (`"Int"`/`"Bool"`/`"String"` → [`Literal`](super::analysed::Literal); unknown → an error).
fn resolve_type_dec(
    unification_map: &mut UnificationMap,
    dec: &Option<TypeDeclaration>,
    span: SourceSpan,
) -> Result<Type, AnalysisError> {
    match dec {
        Some(dec) => {
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
        None => Ok(Type::Var(unification_map.mint_new_type_var())),
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
    fn let_annotation_mismatch_is_an_error() {
        // Annotating a String value as `Int` must be a type error.
        let errors = analyse_src("let x: Int = \"hello\"")
            .expect_err("String value annotated Int is a type error");
        assert!(matches!(errors[0], AnalysisError::TypeMismatch { .. }));
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
        let err = get_type_after_applying_arguments(
            &mut UnificationMap::new(),
            &fn_type,
            &[int_arg()],
            SourceSpan::from((0, 0)),
        )
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
        let result = get_type_after_applying_arguments(
            &mut UnificationMap::new(),
            &fn_type,
            &[int_arg()],
            SourceSpan::from((0, 0)),
        )
        .expect("applying a matching argument should succeed");
        assert_eq!(result, Type::Literal(Literal::Int));
    }

    #[test]
    fn function_parameter_presence_mismatch_is_an_error() {
        // Unifying an argumentless function `Fn(None, _)` with a one-parameter function
        // `Fn(Some(Int), _)` is a shape mismatch — one takes a parameter, the other doesn't.
        let nullary = Type::Fn(None, Box::new(Type::Unit));
        let unary = Type::Fn(
            Some(Box::new(Type::Literal(Literal::Int))),
            Box::new(Type::Unit),
        );
        let err = unify(
            &mut UnificationMap::new(),
            &nullary,
            &unary,
            SourceSpan::from((0, 0)),
        )
        .expect_err("param-presence mismatch is an error");
        assert!(matches!(
            err,
            AnalysisError::FunctionParameterMismatch { .. }
        ));
    }
}
