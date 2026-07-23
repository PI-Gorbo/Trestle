//! Union-find over type variables: the substitution engine behind type inference.
//!
//! [`UnificationMap`] holds the disjoint-set forest of type variables; [`unify`] reconciles two
//! [`Type`]s, solving variables against each other and against concrete types. Everything else in
//! this module (`UnionNode`, the error structs) is an implementation detail that stays private —
//! only [`UnificationMap`], a handful of its methods, and [`unify`] leak to the rest of the pass.

use miette::SourceSpan;

use super::error::TypeCheckError;
use super::typed_ast::{Type, TypeVarId};

enum UnionNode {
    Reference(TypeVarId),
    RootUnionNode(RootUnionNode),
}

enum RootUnionNode {
    FreeTypeVariable,
    Concrete(Type),
}

pub(super) struct UnificationMap {
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
    pub(super) fn new() -> UnificationMap {
        UnificationMap { map: Vec::new() }
    }

    fn find_root(&self, var_id: TypeVarId) -> Option<(TypeVarId, &RootUnionNode)> {
        self.map.get(var_id.0).and_then(|found| match found {
            UnionNode::Reference(type_var_id) => self.find_root(*type_var_id),
            UnionNode::RootUnionNode(root) => Some((var_id, root)),
        })
    }

    pub(super) fn subsitute(&self, ty: &Type) -> Type {
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
    pub(super) fn representative(&self, ty: &Type) -> Type {
        match ty {
            Type::Var(id) => match self.find_root(*id) {
                Some((_, RootUnionNode::Concrete(concrete))) => concrete.clone(),
                Some((root_id, RootUnionNode::FreeTypeVariable)) => Type::Var(root_id),
                None => ty.clone(),
            },
            _ => ty.clone(),
        }
    }

    pub(super) fn mint_new_type_var(&mut self) -> TypeVarId {
        let new_type_var = TypeVarId(self.map.len());
        self.map
            .push(UnionNode::RootUnionNode(RootUnionNode::FreeTypeVariable));

        new_type_var
    }

    fn set(
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

    fn union_with_concrete_type(
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

    fn union_vars(
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

/// Reconcile two types, solving type variables and returning `()` on success or a
/// [`TypeCheckError`] at `span`. Descends into `Fn` children and delegates variable-vs-concrete /
/// variable-vs-variable cases to the union-find on [`UnificationMap`].
pub(super) fn unify(
    unification_map: &mut UnificationMap,
    found: &Type,
    expected: &Type,
    span: SourceSpan,
) -> Result<(), TypeCheckError> {
    match (expected, found) {
        (Type::Var(expected_type_var_id), Type::Var(found_type_var_id)) => unification_map
            .union_vars(*expected_type_var_id, *found_type_var_id)
            .map_err(|union_err| unify_error_to_type_check_error(union_err, span)),
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
                    Err(TypeCheckError::FunctionParameterMismatch {
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
            .map_err(|union_err| unify_error_to_type_check_error(union_err, span)),
        (Type::Var(type_var_id), Type::Literal(_)) => unification_map
            .union_with_concrete_type(*type_var_id, found)
            .map_err(|union_err| unify_error_to_type_check_error(union_err, span)),
        (Type::Var(type_var_id), Type::Fn(_, _)) => unification_map
            .union_with_concrete_type(*type_var_id, found)
            .map_err(|union_err| unify_error_to_type_check_error(union_err, span)),
        (Type::Var(type_var_id), Type::Unit) => unification_map
            .union_with_concrete_type(*type_var_id, found)
            .map_err(|union_err| unify_error_to_type_check_error(union_err, span)),
        (Type::Fn(_, _), Type::Var(type_var_id)) => unification_map
            .union_with_concrete_type(*type_var_id, expected)
            .map_err(|union_err| unify_error_to_type_check_error(union_err, span)),

        (Type::Literal(_), Type::Unit) => create_type_mismatch_error(found, expected, span),
        (Type::Literal(_), Type::Fn(_, _)) => create_type_mismatch_error(found, expected, span),
        (Type::Fn(_, _), Type::Unit) => create_type_mismatch_error(found, expected, span),
        (Type::Fn(_, _), Type::Literal(_)) => create_type_mismatch_error(found, expected, span),
        (Type::Unit, Type::Literal(_)) => create_type_mismatch_error(found, expected, span),
        (Type::Unit, Type::Var(_)) => create_type_mismatch_error(found, expected, span),
        (Type::Unit, Type::Fn(_, _)) => create_type_mismatch_error(found, expected, span),
    }
}

/// Translate a low-level [`UnifyError`] into the user-facing [`TypeCheckError`] at `span`.
fn unify_error_to_type_check_error(union_err: UnifyError, span: SourceSpan) -> TypeCheckError {
    match union_err {
        UnifyError::TypeMismatch(type_mismatch) => TypeCheckError::TypeMismatch {
            expected: type_mismatch.expected,
            found: type_mismatch.found,
            span,
        },
        UnifyError::FreeTypeVariableNotFoundError(err) => TypeCheckError::InternalError {
            message: format!(
                "type variable {} was referenced during unification but never minted",
                err.type_variable_id.0
            ),
            span,
        },
    }
}

fn create_type_mismatch_error(
    found: &Type,
    expected: &Type,
    span: SourceSpan,
) -> Result<(), TypeCheckError> {
    Err(TypeCheckError::TypeMismatch {
        expected: expected.clone(),
        found: found.clone(),
        span,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::type_check::typed_ast::Literal;

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
            TypeCheckError::FunctionParameterMismatch { .. }
        ));
    }
}
