use std::cmp::min;
use std::collections::{BTreeMap, BTreeSet};

use miette::SourceSpan;

use crate::type_check::inference::LetGeneralisationLevel;
use crate::type_check::typed_ast::RecordTail::{self, Closed};

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
    missing: BTreeSet<String>,

    additional: BTreeSet<String>,
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
            Some((_, RootUnionNode::Concrete(concrete))) => self.resolve_shallow(concrete),
            Some((root_id, RootUnionNode::FreeTypeVariable(_))) => Type::Var(root_id),
            None => ty.clone(),
        })
    }

    /// Records that the first type is equivalent to the second type.
    pub(super) fn unify(
        &mut self,
        expected: &Type,
        found: &Type,
        span: SourceSpan,
    ) -> Result<(), TypeCheckError> {
        self.unify_inner(expected, found)
            .map_err(|err| err.into_type_check_error(span))
    }

    fn unify_inner(&mut self, expected: &Type, found: &Type) -> Result<(), UnifyError> {
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
                        self.unify_inner(expected_param, found_param)
                    }
                };

                param_unification?;
                self.unify_inner(&expected_body, &found_body)
            }

            (
                Type::Record {
                    fields: expected_fields,
                    tail: expected_tail,
                },
                Type::Record {
                    fields: found_fields,
                    tail: found_tail,
                },
            ) => {
                match (expected_tail, found_tail) {
                    // Two records must exact match if they are both closed.
                    // Eg:
                    // type Point = { x: Int, y: Int }
                    // let p: Point = { x: 3, y: 4 }
                    (RecordTail::Closed, RecordTail::Closed) => {
                        let missing: BTreeSet<String> = expected_fields
                            .keys()
                            .filter(|name| !found_fields.contains_key(*name))
                            .cloned()
                            .collect();

                        let additional: BTreeSet<String> = found_fields
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
                            self.unify_inner(expected_field, found_field)?;
                        }

                        Ok(())
                    }

                    // We expect a closed record, but we receive an open one.
                    (RecordTail::Closed, RecordTail::Open(found_tail)) => {
                        // error example:
                        // expected: { x: Int, y: Int }      // closed
                        // found:    { z: a | r }            // open
                        let any_missing_fields_in_found_record = found_fields
                            .keys()
                            .any(|name| !expected_fields.contains_key(name));

                        if any_missing_fields_in_found_record {
                            return Err(UnifyError::RecordFieldMismatch(RecordFieldMismatch {
                                // The open tail can supply anything it lacks, so only its
                                // extra fields are an error.
                                missing: BTreeSet::new(),
                                additional: found_fields
                                    .keys()
                                    .filter(|name| !expected_fields.contains_key(*name))
                                    .cloned()
                                    .collect(),
                            }));
                        }

                        // No extra fields, so every found field is a common field.
                        for (name, found_field) in &found_fields {
                            self.unify_inner(&expected_fields[name], found_field)?;
                        }

                        // We now know that the tail is a closed record (in this instance).
                        // expected: { x: Int, y: Int, z: Bool }     // closed
                        // found:    { x: a | r }                    // open
                        // We would unify the tail with { y: Int, z: Bool }
                        let extension_of_row = Type::Record {
                            fields: expected_fields
                                .into_iter()
                                .filter(|(name, _)| !found_fields.contains_key(name))
                                .collect(),
                            tail: Closed,
                        };

                        self.unify_inner(&extension_of_row, &Type::Var(found_tail))
                    }

                    // We have an open record but received a closed one.
                    // Eg:
                    // let getField = (rec) => rec.field
                    // getField({field: "abc", otherField: 123 })
                    (RecordTail::Open(expected_tail), RecordTail::Closed) => {
                        // error example:
                        // expected: { z: Int | r }            // open
                        // found: { x: Int, y: Int }           // closed
                        let any_missing_fields_from_expected = expected_fields
                            .keys()
                            .any(|name| !found_fields.contains_key(name));

                        if any_missing_fields_from_expected {
                            return Err(UnifyError::RecordFieldMismatch(RecordFieldMismatch {
                                missing: expected_fields
                                    .keys()
                                    .filter(|name| !found_fields.contains_key(*name))
                                    .cloned()
                                    .collect(),
                                // The open tail absorbs the closed record's extra fields.
                                additional: BTreeSet::new(),
                            }));
                        }

                        // No extra fields, so every found field is a common field.
                        for (name, expected_field) in &expected_fields {
                            self.unify_inner(expected_field, &found_fields[name])?;
                        }

                        // We now know that the tail is a closed record (in this instance).
                        // expected:    { x: a | r }                    // open
                        // found:       { x: Int, y: Int, z: Bool }     // closed
                        let extension_of_row = Type::Record {
                            fields: found_fields
                                .into_iter()
                                .filter(|(name, _)| !expected_fields.contains_key(name))
                                .collect(),
                            tail: Closed,
                        };

                        self.unify_inner(&Type::Var(expected_tail), &extension_of_row)
                    }

                    // Both records are open: each side knows some fields, and the rest is unknown.
                    // Eg: using two fields of the same parameter
                    // let getBoth = (rec) => [rec.x, rec.y]
                    // rec.x says rec: { x: a | r1 }, and rec.y says rec: { y: b | r2 }
                    //
                    // expected: { x: a | r1 }
                    // found:    { y: b | r2 }
                    // Each tail must supply the fields only the other side has,
                    // and both must end in the same fresh tail r3:
                    // r1 := { y: b | r3 }   so expected = { x: a, y: b | r3 }
                    // r2 := { x: a | r3 }   so found    = { x: a, y: b | r3 }
                    (RecordTail::Open(expected_tail), RecordTail::Open(found_tail)) => {
                        // Unify the common fields first, while both maps are still borrowed.
                        for (name, expected_field) in &expected_fields {
                            if let Some(found_field) = found_fields.get(name) {
                                self.unify_inner(expected_field, found_field)?;
                            }
                        }

                        let missing_from_found: BTreeMap<String, Box<Type>> = expected_fields
                            .iter()
                            .filter(|(name, _)| !found_fields.contains_key(*name))
                            .map(|(name, ty)| (name.clone(), ty.clone()))
                            .collect();

                        let missing_from_expected: BTreeMap<String, Box<Type>> = found_fields
                            .into_iter()
                            .filter(|(name, _)| !expected_fields.contains_key(name))
                            .collect();

                        // { x | r } ~ { y | r }: one row can't gain different fields.
                        let expected_root = self.find_root(expected_tail).map(|(root, _)| root);
                        let found_root = self.find_root(found_tail).map(|(root, _)| root);
                        if expected_root == found_root {
                            if missing_from_found.is_empty() && missing_from_expected.is_empty() {
                                return Ok(());
                            }
                            return Err(UnifyError::RecordFieldMismatch(RecordFieldMismatch {
                                missing: missing_from_found.into_keys().collect(),
                                additional: missing_from_expected.into_keys().collect(),
                            }));
                        }

                        // None = the tail is already solved to a record.
                        let expected_tail_level = self
                            .get_let_generalisation_level(expected_tail)
                            .ok_or_else(|| -> UnifyError {
                                todo!("flatten records whose tail is already solved")
                            })?;
                        let found_tail_level = self
                            .get_let_generalisation_level(found_tail)
                            .ok_or_else(|| -> UnifyError {
                                todo!("flatten records whose tail is already solved")
                            })?;

                        // One shared tail, so both rows stay the same row from here on.
                        let rest =
                            self.mint_new_type_var(min(expected_tail_level, found_tail_level));

                        // Each tail absorbs the fields only the other side has.
                        let extension_of_expected = Type::Record {
                            fields: missing_from_expected,
                            tail: RecordTail::Open(rest),
                        };
                        self.unify_inner(&Type::Var(expected_tail), &extension_of_expected)?;

                        let extension_of_found = Type::Record {
                            fields: missing_from_found,
                            tail: RecordTail::Open(rest),
                        };
                        self.unify_inner(&extension_of_found, &Type::Var(found_tail))
                    }
                }
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
                Some((root2, RootUnionNode::FreeTypeVariable(_))) => root == root2,
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
            Type::Record { fields, tail } => {
                fields
                    .values()
                    .any(|field| self.root_occurs_in_type(root, field))
                    || matches!(tail, RecordTail::Open(tail_var)
                        if self.root_occurs_in_type(root, &Type::Var(*tail_var)))
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
                )
                .map_err(UnifyError::FreeTypeVariableNotFoundError)?;

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
