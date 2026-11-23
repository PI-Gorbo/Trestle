use std::cmp::min;

use miette::SourceSpan;

use crate::type_check::inference::LetGeneralisationLevel;

use super::error::TypeCheckError;
use super::typed_ast::{Type, TypeVarId};

enum UnionNode {
    Reference(TypeVarId),
    RootUnionNode(RootUnionNode),
}

enum RootUnionNode {
    FreeTypeVariable(LetGeneralisationLevel),
    Concrete(Type),
}

pub(super) struct UnificationMap {
    map: Vec<UnionNode>,
}

#[derive(Debug)]
struct FreeTypeVariableNotFoundError {
    type_variable_id: TypeVarId,
}

#[derive(Debug)]
struct TypeMismatch {
    expected: Type,
    found: Type,
}

#[derive(Debug)]
struct InfiniteType {
    var: TypeVarId,
    ty: Type,
}

#[derive(Debug)]
struct RecordFieldMismatch {
    missing: Vec<String>,

    additional: Vec<String>,
}

#[derive(Debug)]
enum UnifyError {
    FunctionParameterNotProvided(TypeMismatch),
    FunctionParameterNotNeeded(TypeMismatch),
    TypeMismatch(TypeMismatch),
    RecordFieldMismatch(RecordFieldMismatch),
    FreeTypeVariableNotFoundError(FreeTypeVariableNotFoundError),
    InfiniteType(InfiniteType),
}

impl UnifyError {
    fn into_type_check_error(self, span: SourceSpan) -> TypeCheckError {
        match self {
            UnifyError::FunctionParameterNotProvided(mismatch)
            | UnifyError::FunctionParameterNotNeeded(mismatch) => {
                TypeCheckError::FunctionParameterMismatch {
                    expected: mismatch.expected,
                    found: mismatch.found,
                    span,
                }
            }

            UnifyError::TypeMismatch(mismatch) => TypeCheckError::TypeMismatch {
                expected: mismatch.expected,
                found: mismatch.found,
                span,
            },

            UnifyError::RecordFieldMismatch(mismatch) => TypeCheckError::RecordFieldMismatch {
                missing: mismatch.missing,
                additional: mismatch.additional,
                span,
            },

            UnifyError::FreeTypeVariableNotFoundError(err) => TypeCheckError::InternalError {
                message: format!(
                    "type variable {} was referenced during unification but never minted",
                    err.type_variable_id.0
                ),
                span,
            },

            UnifyError::InfiniteType(infinite_type) => TypeCheckError::InfiniteType {
                var: infinite_type.var,
                ty: infinite_type.ty,
                span,
            },
        }
    }
}

impl UnificationMap {
    pub(super) fn new() -> UnificationMap {
        UnificationMap { map: Vec::new() }
    }

    pub(super) fn mint_new_type_var(&mut self, level: LetGeneralisationLevel) -> TypeVarId {
        let new_type_var = TypeVarId(self.map.len());
        self.map
            .push(UnionNode::RootUnionNode(RootUnionNode::FreeTypeVariable(
                level,
            )));

        new_type_var
    }

    pub(super) fn get_let_generalisation_level(
        &self,
        id: TypeVarId,
    ) -> Option<LetGeneralisationLevel> {
        match self.find_root(id)? {
            (_, RootUnionNode::FreeTypeVariable(level)) => Some(*level),
            (_, RootUnionNode::Concrete(_)) => None,
        }
    }

    /// Walks through a given type as a tree, and resolves all type vars
    /// to the root type
    pub(super) fn resolve(&self, ty: &Type) -> Type {
        ty.map_free_type_variables(&mut |type_var_id| match self.find_root(*type_var_id) {
            Some((_, RootUnionNode::Concrete(concrete))) => self.resolve(concrete),
            Some((root_id, RootUnionNode::FreeTypeVariable(_))) => Type::Var(root_id),
            None => ty.clone(),
        })
    }

    /// If a type variable, resolves to the root of representation of that type
    /// Otherwise, just returns the type
    pub(super) fn resolve_shallow(&self, ty: &Type) -> Type {
        ty.map_free_type_variables(&mut |id| match self.find_root(*id) {
            Some((_, RootUnionNode::Concrete(concrete))) => concrete.clone(),
            Some((root_id, RootUnionNode::FreeTypeVariable(_))) => Type::Var(root_id),
            None => ty.clone(),
        })
    }

    /// Records that the first type is equivalent to the second type.
    pub(super) fn unify(
        &mut self,
        first: &Type,
        second: &Type,
        span: SourceSpan,
    ) -> Result<(), TypeCheckError> {
        self.unify_inner(first, second)
            .map_err(|err| err.into_type_check_error(span))
    }

    fn unify_inner(&mut self, found: &Type, expected: &Type) -> Result<(), UnifyError> {
        match (self.resolve_shallow(expected), self.resolve_shallow(found)) {
            (Type::Var(var1), Type::Var(var2)) => self.union_vars(var1, var2),

            (Type::Var(type_var_id), concrete_type) | (concrete_type, Type::Var(type_var_id)) => {
                self.union_with_concrete_type(type_var_id, &concrete_type)
            }

            (Type::Unit, Type::Unit) => Ok(()),

            (Type::Literal(literal_a), Type::Literal(literal_b)) => match literal_a == literal_b {
                true => Ok(()),

                false => Err(UnifyError::TypeMismatch(TypeMismatch {
                    expected: Type::Literal(literal_a),
                    found: Type::Literal(literal_b),
                })),
            },

            (
                Type::Fn {
                    parameter: expected_param,
                    return_type: expected_body,
                },
                Type::Fn {
                    parameter: found_param,
                    return_type: found_body,
                },
            ) => {
                let shape_mismatch = || TypeMismatch {
                    expected: Type::Fn {
                        parameter: expected_param.clone(),
                        return_type: expected_body.clone(),
                    },
                    found: Type::Fn {
                        parameter: found_param.clone(),
                        return_type: found_body.clone(),
                    },
                };

                let param_unification = match (&expected_param, &found_param) {
                    (None, Some(_)) => {
                        Err(UnifyError::FunctionParameterNotNeeded(shape_mismatch()))
                    }
                    (Some(_), None) => {
                        Err(UnifyError::FunctionParameterNotProvided(shape_mismatch()))
                    }
                    (None, None) => Ok(()),
                    (Some(expected_param), Some(found_param)) => {
                        self.unify_inner(found_param, expected_param)
                    }
                };

                param_unification?;
                self.unify_inner(&found_body, &expected_body)
            }

            (
                Type::Record {
                    fields: expected_fields,
                    tail,
                },
                Type::Record {
                    fields: found_fields,
                    tail,
                },
            ) => {
                let missing: Vec<String> = expected_fields
                    .keys()
                    .filter(|name| !found_fields.contains_key(*name))
                    .cloned()
                    .collect();

                let additional: Vec<String> = found_fields
                    .keys()
                    .filter(|name| !expected_fields.contains_key(*name))
                    .cloned()
                    .collect();

                if !missing.is_empty() || !additional.is_empty() {
                    return Err(UnifyError::RecordFieldMismatch(RecordFieldMismatch {
                        missing,
                        additional,
                    }));
                }

                for (name, expected_field) in &expected_fields {
                    let found_field = &found_fields[name];
                    self.unify_inner(found_field, expected_field)?;
                }

                Ok(())
            }

            (expected, found) => Err(UnifyError::TypeMismatch(TypeMismatch { expected, found })),
        }
    }

    fn find_root(&self, var_id: TypeVarId) -> Option<(TypeVarId, &RootUnionNode)> {
        self.map.get(var_id.0).and_then(|found| match found {
            UnionNode::Reference(type_var_id) => self.find_root(*type_var_id),
            UnionNode::RootUnionNode(root) => Some((var_id, root)),
        })
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

    fn root_occurs_in_type(&self, root: TypeVarId, declared_type: &Type) -> bool {
        match declared_type {
            Type::Unit | Type::Literal(_) => false,
            Type::Var(type_var_id2) => match self.find_root(*type_var_id2) {
                None => false,
                Some((root2, RootUnionNode::FreeTypeVariable(level))) => root == root2,
                Some((root2, RootUnionNode::Concrete(declared_type))) => {
                    root == root2 || self.root_occurs_in_type(root, declared_type)
                }
            },
            Type::Fn {
                parameter: param,
                return_type: body,
            } => {
                param
                    .as_ref()
                    .is_some_and(|p| self.root_occurs_in_type(root, p))
                    || self.root_occurs_in_type(root, body)
            }
            Type::Record(btree_map) => {
                btree_map
                    .into_iter()
                    .fold(false, |occurs, (_, record_key_type)| {
                        occurs || self.root_occurs_in_type(root, record_key_type)
                    })
            }
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

        let root_concrete_type = match root_node {
            RootUnionNode::Concrete(ty) => Some(ty.clone()),
            RootUnionNode::FreeTypeVariable(_) => None,
        };

        match root_concrete_type {
            Some(root_concrete_type) => self.unify_inner(concrete_type, &root_concrete_type),

            None => {
                if self.root_occurs_in_type(root_id, concrete_type) {
                    return Err(UnifyError::InfiniteType(InfiniteType {
                        var: root_id,

                        ty: self.resolve(concrete_type),
                    }));
                }
                self.set(
                    root_id,
                    UnionNode::RootUnionNode(RootUnionNode::Concrete(concrete_type.clone())),
                )
                .map_err(UnifyError::FreeTypeVariableNotFoundError)
            }
        }
    }

    fn union_vars(
        &mut self,
        first_var_id: TypeVarId,
        second_var_id: TypeVarId,
    ) -> Result<(), UnifyError> {
        if first_var_id == second_var_id {
            return Ok(());
        }

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
            // Equating two free type variables.
            // We must RAISE the surviving root to the min of the two LEVELS.
            // This is so we don't generalise a free type variable that is pointing towards
            // another that cannot be generalised.
            (
                RootUnionNode::FreeTypeVariable(first_level),
                RootUnionNode::FreeTypeVariable(second_level),
            ) => {
                let updated_let_generalisation_level = *min(first_level, second_level);

                self.set(
                    second_found_root_id,
                    UnionNode::Reference(first_found_root_id),
                )
                .map_err(UnifyError::FreeTypeVariableNotFoundError)?;

                self.set(
                    first_found_root_id,
                    UnionNode::RootUnionNode(RootUnionNode::FreeTypeVariable(
                        updated_let_generalisation_level,
                    )),
                );

                Ok(())
            }

            // Free <-> Concrete.
            // Point the free type to the concrete type.
            (RootUnionNode::FreeTypeVariable(_), RootUnionNode::Concrete(_)) => {
                self.set(
                    first_found_root_id,
                    UnionNode::Reference(second_found_root_id),
                )
                .map_err(UnifyError::FreeTypeVariableNotFoundError)?;
                Ok(())
            }
            (RootUnionNode::Concrete(_), RootUnionNode::FreeTypeVariable(_)) => {
                self.set(
                    second_found_root_id,
                    UnionNode::Reference(first_found_root_id),
                )
                .map_err(UnifyError::FreeTypeVariableNotFoundError)?;
                Ok(())
            }

            // Concrete <-> Concrete
            // This doesn't make sense. Can't unify two concrete types
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::type_check::typed_ast::Literal;

    #[test]
    fn function_parameter_presence_mismatch_is_an_error() {
        let nullary = Type::Fn {
            parameter: None,
            return_type: Box::new(Type::Unit),
        };
        let unary = Type::Fn {
            parameter: Some(Box::new(Type::Literal(Literal::Int))),
            return_type: Box::new(Type::Unit),
        };
        let mut map = UnificationMap::new();
        let err = map
            .unify(&nullary, &unary, SourceSpan::from((0, 0)))
            .expect_err("param-presence mismatch is an error");
        assert!(matches!(
            err,
            TypeCheckError::FunctionParameterMismatch { .. }
        ));
    }

    #[test]
    fn a_solved_variable_unifies_structurally_with_a_compatible_function() {
        let mut unification_map = UnificationMap::new();
        let v0 = Type::Var(unification_map.mint_new_type_var(LetGeneralisationLevel::new()));
        let v1 = Type::Var(unification_map.mint_new_type_var(LetGeneralisationLevel::new()));
        let span = SourceSpan::from((0, 0));

        let partially_known = Type::Fn {
            parameter: Some(Box::new(v1.clone())),
            return_type: Box::new(Type::Literal(Literal::Int)),
        };
        unification_map
            .unify(&partially_known, &v0, span)
            .expect("binding a fresh variable to a concrete type succeeds");

        let fully_known = Type::Fn {
            parameter: Some(Box::new(Type::Literal(Literal::Int))),
            return_type: Box::new(Type::Literal(Literal::Int)),
        };
        unification_map
            .unify(&fully_known, &v0, span)
            .expect("a solved variable unifies structurally with a compatible function");

        assert_eq!(
            unification_map.resolve(&v1),
            Type::Literal(Literal::Int),
            "the partially known parameter should have been solved to Int"
        );
    }

    #[test]
    fn binding_a_variable_into_a_type_that_contains_it_is_an_infinite_type() {
        let mut unification_map = UnificationMap::new();
        let v0 = Type::Var(unification_map.mint_new_type_var(LetGeneralisationLevel::new()));
        let span = SourceSpan::from((0, 0));

        let recursive = Type::Fn {
            parameter: Some(Box::new(v0.clone())),
            return_type: Box::new(Type::Unit),
        };
        let err = unification_map
            .unify(&recursive, &v0, span)
            .expect_err("binding a variable into a type containing it is an infinite type");

        match err {
            TypeCheckError::InfiniteType {
                var,
                ty,
                span: err_span,
            } => {
                assert_eq!(Type::Var(var), v0, "the variable that would swallow itself");
                assert_eq!(ty, recursive, "the reported type keeps its real shape");
                assert_eq!(err_span, span, "the caller's span survives");
            }
            other => panic!("expected an infinite type error, got {other:?}"),
        }

        assert_eq!(unification_map.resolve(&v0), v0);
    }

    #[test]
    fn the_occurs_check_sees_through_an_equivalence_class() {
        let mut unification_map = UnificationMap::new();
        let v0 = Type::Var(unification_map.mint_new_type_var(LetGeneralisationLevel::new()));
        let v1 = Type::Var(unification_map.mint_new_type_var(LetGeneralisationLevel::new()));
        let span = SourceSpan::from((0, 0));

        unification_map
            .unify(&v0, &v1, span)
            .expect("unioning two free variables succeeds");

        let recursive = Type::Fn {
            parameter: Some(Box::new(v0.clone())),
            return_type: Box::new(Type::Unit),
        };
        let err = unification_map
            .unify(&recursive, &v1, span)
            .expect_err("v1 := Fn(v0, Unit) is a cycle because v0 and v1 share a root");

        match err {
            TypeCheckError::InfiniteType { var, .. } => assert_eq!(
                Type::Var(var),
                unification_map.resolve(&v1),
                "the error names the class's canonical root"
            ),
            other => panic!("expected an infinite type error, got {other:?}"),
        }

        assert_eq!(unification_map.resolve(&v0), unification_map.resolve(&v1));
    }

    #[test]
    fn unifying_a_variable_with_itself_is_a_no_op() {
        let mut unification_map = UnificationMap::new();
        let v: Type = Type::Var(unification_map.mint_new_type_var(LetGeneralisationLevel::new()));
        let span = SourceSpan::from((0, 0));

        unification_map
            .unify(&v, &v, span)
            .expect("unifying a variable with itself succeeds");

        assert_eq!(unification_map.resolve(&v), v);
    }
}
