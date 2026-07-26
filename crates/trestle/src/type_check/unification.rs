//! Union-find over type variables: the substitution engine behind type inference.
//!
//! [`UnificationMap`] holds the disjoint-set forest of type variables; [`UnificationMap::unify`]
//! reconciles two [`Type`]s, solving variables against each other and against concrete types.
//! Everything else in this module (`UnionNode`, `UnifyError`, the error structs) is an
//! implementation detail that stays private — only [`UnificationMap`] and a handful of its
//! methods leak to the rest of the pass.

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
enum UnifyError {
    FunctionParameterNotProvided(TypeMismatch),
    FunctionParameterNotNeeded(TypeMismatch),
    TypeMismatch(TypeMismatch),
    FreeTypeVariableNotFoundError(FreeTypeVariableNotFoundError),
}

impl UnifyError {
    /// Attach the caller's span, turning the span-free failure the union-find reports into the
    /// user-facing diagnostic. Unification itself has no idea *where* the two types came from.
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

            // A `Var` that was never minted means inference handed us an id from nowhere; that is
            // a compiler bug, not a user error.
            UnifyError::FreeTypeVariableNotFoundError(err) => TypeCheckError::InternalError {
                message: format!(
                    "type variable {} was referenced during unification but never minted",
                    err.type_variable_id.0
                ),
                span,
            },
        }
    }
}

impl UnificationMap {
    pub(super) fn new() -> UnificationMap {
        UnificationMap { map: Vec::new() }
    }

    pub(super) fn mint_new_type_var(&mut self) -> TypeVarId {
        let new_type_var = TypeVarId(self.map.len());
        self.map
            .push(UnionNode::RootUnionNode(RootUnionNode::FreeTypeVariable));

        new_type_var
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

    /// Reconcile two types, solving type variables and returning `()` on success or a
    /// [`TypeCheckError`] at `span`.
    pub(super) fn unify(
        &mut self,
        found: &Type,
        expected: &Type,
        span: SourceSpan,
    ) -> Result<(), TypeCheckError> {
        self.unify_inner(found, expected)
            .map_err(|err| err.into_type_check_error(span))
    }

    /// The recursive core of [`UnificationMap::unify`], span-free so the recursion doesn't have to
    /// carry one. Both sides are resolved to their representatives *before* dispatching — that is
    /// what lets a variable already solved to a partially known `Fn` unify structurally with a
    /// more concrete one. Descends into `Fn` children and delegates variable-vs-concrete /
    /// variable-vs-variable cases to the union-find.
    fn unify_inner(&mut self, found: &Type, expected: &Type) -> Result<(), UnifyError> {
        match (self.representative(expected), self.representative(found)) {
            (Type::Var(var1), Type::Var(var2)) => self.union_vars(var1, var2),

            (Type::Var(type_var_id), concrete_type) | (concrete_type, Type::Var(type_var_id)) => {
                self.union_with_concrete_type(type_var_id, &concrete_type)
            }

            (Type::Unit, Type::Unit) => Ok(()),

            (Type::Literal(literal_a), Type::Literal(literal_b)) => match literal_a == literal_b {
                true => Ok(()),
                // Report the *resolved* literals; `expected`/`found` may still be `Var`s.
                false => Err(UnifyError::TypeMismatch(TypeMismatch {
                    expected: Type::Literal(literal_a),
                    found: Type::Literal(literal_b),
                })),
            },

            (Type::Fn(expected_param, expected_body), Type::Fn(found_param, found_body)) => {
                let shape_mismatch = || TypeMismatch {
                    expected: Type::Fn(expected_param.clone(), expected_body.clone()),
                    found: Type::Fn(found_param.clone(), found_body.clone()),
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

            (expected, found) => Err(UnifyError::TypeMismatch(TypeMismatch { expected, found })),
        }
    }

    // Given a type var, return the root of the equivalence class it is apart of.
    // Returns the id of this root node, and the node.
    fn find_root(&self, var_id: TypeVarId) -> Option<(TypeVarId, &RootUnionNode)> {
        self.map.get(var_id.0).and_then(|found| match found {
            UnionNode::Reference(type_var_id) => self.find_root(*type_var_id),
            UnionNode::RootUnionNode(root) => Some((var_id, root)),
        })
    }

    // Given a type, return its root / most canonical representation.
    // If its a concrete type, then we don't need to find anything. It is its most canonical representation.
    // If it is a type variable, then we follow the unification map to its representation.
    //
    // This is *shallow*: a concrete root may still contain variables of its own (e.g. the parameter
    // of an `Fn`), so callers walking a type have to re-resolve as they descend.
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

        // `find_root` hands back a reference into `self.map`; clone the root's type out so that
        // borrow ends before the `&mut self` recursion / `set` below.
        let root_concrete_type = match root_node {
            RootUnionNode::Concrete(ty) => Some(ty.clone()),
            RootUnionNode::FreeTypeVariable => None,
        };

        match root_concrete_type {
            // The variable is already solved — reconcile the two concrete types structurally.
            Some(root_concrete_type) => self.unify_inner(concrete_type, &root_concrete_type),
            // Still free — bind it.
            None => self
                .set(
                    root_id,
                    UnionNode::RootUnionNode(RootUnionNode::Concrete(concrete_type.clone())),
                )
                .map_err(UnifyError::FreeTypeVariableNotFoundError),
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
        let mut map = UnificationMap::new();
        let err = map
            .unify(&nullary, &unary, SourceSpan::from((0, 0)))
            .expect_err("param-presence mismatch is an error");
        assert!(matches!(
            err,
            TypeCheckError::FunctionParameterMismatch { .. }
        ));
    }

    /// Regression: once a variable's root is `Concrete`, `union_with_concrete_type` compares the
    /// two concrete types with `PartialEq` instead of unifying them structurally. So a root holding
    /// a *partially known* `Fn(v1, Int)` is reported as mismatching `Fn(Int, Int)` rather than
    /// solving `v1 := Int`. Records make this constant — they are the first type that routinely
    /// holds variables *and* sits behind one — so `unify` must resolve both sides to their
    /// representatives before dispatching.
    #[test]
    fn a_solved_variable_unifies_structurally_with_a_compatible_function() {
        let mut unification_map = UnificationMap::new();
        let v0 = Type::Var(unification_map.mint_new_type_var());
        let v1 = Type::Var(unification_map.mint_new_type_var());
        let span = SourceSpan::from((0, 0));

        // v0 := Fn(v1, Int) — a function whose parameter type is still unknown.
        let partially_known = Type::Fn(
            Some(Box::new(v1.clone())),
            Box::new(Type::Literal(Literal::Int)),
        );
        unification_map
            .unify(&partially_known, &v0, span)
            .expect("binding a fresh variable to a concrete type succeeds");

        // Unifying that against a fully concrete `Fn(Int, Int)` must descend and solve v1.
        let fully_known = Type::Fn(
            Some(Box::new(Type::Literal(Literal::Int))),
            Box::new(Type::Literal(Literal::Int)),
        );
        unification_map
            .unify(&fully_known, &v0, span)
            .expect("a solved variable unifies structurally with a compatible function");

        assert_eq!(
            unification_map.subsitute(&v1),
            Type::Literal(Literal::Int),
            "the partially known parameter should have been solved to Int"
        );
    }

    #[test]
    fn unifying_a_variable_with_itself_is_a_no_op() {
        let mut unification_map = UnificationMap::new();
        let v: Type = Type::Var(unification_map.mint_new_type_var());
        let span = SourceSpan::from((0, 0));

        unification_map
            .unify(&v, &v, span)
            .expect("unifying a variable with itself succeeds");

        // Reaching this line at all is the test; the variable should still be free.
        assert_eq!(unification_map.subsitute(&v), v);
    }
}
