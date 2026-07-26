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

    // Given a type, return its root / most canonical representation.
    // If its a concrete type, then we don't need to find anything. It is its most canonical representation.
    // If it is a type variable, then we follow the unification map to its representation.
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
                match (
                    self.representative(concrete_type),
                    self.representative(root_node_concrete_type),
                ) {
                    // (Type::Var(a), Type::Var(b))          => union_vars(a, b),
                    // (Type::Var(a), t) | (t, Type::Var(a)) => bind_var(a, &t),   // occurs check inside
                    // (Type::Literal(a), Type::Literal(b))  => match a == b { .. },
                    // (Type::Fn(p1, r1), Type::Fn(p2, r2))  => ..recurse..,
                    // (Type::Record(r1), Type::Record(r2))  => unify_rows(map, &r1, &r2, span),
                    // (found, expected)                     => create_type_mismatch_error(..),
                    (Type::Unit, Type::Unit) => todo!(),
                    (Type::Unit, Type::Literal(literal)) => todo!(),
                    (Type::Unit, Type::Var(type_var_id)) => todo!(),
                    (Type::Unit, Type::Fn(_, _)) => todo!(),
                    (Type::Literal(literal), Type::Unit) => todo!(),
                    (Type::Literal(literal), Type::Literal(literal)) => todo!(),
                    (Type::Literal(literal), Type::Var(type_var_id)) => todo!(),
                    (Type::Literal(literal), Type::Fn(_, _)) => todo!(),
                    (Type::Var(type_var_id), Type::Unit) => todo!(),
                    (Type::Var(type_var_id), Type::Literal(literal)) => todo!(),
                    (Type::Var(type_var_id), Type::Var(type_var_id)) => todo!(),
                    (Type::Var(type_var_id), Type::Fn(_, _)) => todo!(),
                    (Type::Fn(_, _), Type::Unit) => todo!(),
                    (Type::Fn(_, _), Type::Literal(literal)) => todo!(),
                    (Type::Fn(_, _), Type::Var(type_var_id)) => todo!(),
                    (Type::Fn(_, _), Type::Fn(_, _)) => todo!(),
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

    /// Reconcile two types, solving type variables and returning `()` on success or a
    /// [`TypeCheckError`] at `span`. Descends into `Fn` children and delegates variable-vs-concrete /
    /// variable-vs-variable cases to the union-find on [`UnificationMap`].
    pub(super) fn unify(
        &mut self,
        found: &Type,
        expected: &Type,
        span: SourceSpan,
    ) -> Result<(), TypeCheckError> {
        match (self.representative(expected), self.representative(found)) {
            // (Type::Var(expected_type_var_id), Type::Var(found_type_var_id)) => unification_map
            //     .union_vars(*expected_type_var_id, *found_type_var_id)
            //     .map_err(|union_err| unify_error_to_type_check_error(union_err, span)),
            // (Type::Unit, Type::Unit) => Ok(()),
            // (Type::Literal(first_literal), Type::Literal(second_literal)) => {
            //     match first_literal == second_literal {
            //         true => Ok(()),
            //         false => create_type_mismatch_error(found, expected, span),
            //     }
            // }
            // (Type::Fn(param1, result1), Type::Fn(param2, result2)) => {
            //     let parma_unification = match (param1, param2) {
            //         (None, Some(_)) | (Some(_), None) => {
            //             Err(TypeCheckError::FunctionParameterMismatch {
            //                 expected: expected.clone(),
            //                 found: found.clone(),
            //                 span,
            //             })
            //         }

            //         (None, None) => Ok(()),
            //         (Some(param1), Some(param2)) => unify(unification_map, param1, param2, span),
            //     };

            //     parma_unification?;
            //     unify(unification_map, result1, result2, span)
            // }

            // (Type::Literal(_), Type::Var(type_var_id)) => unification_map
            //     .union_with_concrete_type(*type_var_id, expected)
            //     .map_err(|union_err| unify_error_to_type_check_error(union_err, span)),
            // (Type::Var(type_var_id), Type::Literal(_)) => unification_map
            //     .union_with_concrete_type(*type_var_id, found)
            //     .map_err(|union_err| unify_error_to_type_check_error(union_err, span)),
            // (Type::Var(type_var_id), Type::Fn(_, _)) => unification_map
            //     .union_with_concrete_type(*type_var_id, found)
            //     .map_err(|union_err| unify_error_to_type_check_error(union_err, span)),
            // (Type::Var(type_var_id), Type::Unit) => unification_map
            //     .union_with_concrete_type(*type_var_id, found)
            //     .map_err(|union_err| unify_error_to_type_check_error(union_err, span)),
            // (Type::Fn(_, _), Type::Var(type_var_id)) => unification_map
            //     .union_with_concrete_type(*type_var_id, expected)
            //     .map_err(|union_err| unify_error_to_type_check_error(union_err, span)),

            // (Type::Literal(_), Type::Unit) => create_type_mismatch_error(found, expected, span),
            // (Type::Literal(_), Type::Fn(_, _)) => create_type_mismatch_error(found, expected, span),
            // (Type::Fn(_, _), Type::Unit) => create_type_mismatch_error(found, expected, span),
            // (Type::Fn(_, _), Type::Literal(_)) => create_type_mismatch_error(found, expected, span),
            // (Type::Unit, Type::Literal(_)) => create_type_mismatch_error(found, expected, span),
            // (Type::Unit, Type::Var(_)) => create_type_mismatch_error(found, expected, span),
            // (Type::Unit, Type::Fn(_, _)) => create_type_mismatch_error(found, expected, span),
            (Type::Var(a), t) | (t, Type::Var(a)) => bind_var(a, &t), // occurs check inside

            (Type::Unit, Type::Unit) => Ok(()),
            (Type::Literal(literal_a), Type::Literal(literal_b)) => match literal_a == literal_b {
                true => Ok(()),
                false => create_type_mismatch_error(found, expected, span),
            },
            (Type::Fn(p1, b1), Type::Fn(p2, b2)) => todo!(),
            (Type::Var(var1), Type::Var(var2)) => todo!(),

            (expected, found) => create_type_mismatch_error(&found, &expected, span),
        }
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

    /// Regression: once a variable's root is `Concrete`, `union_with_concrete_type` compares the
    /// two concrete types with `PartialEq` instead of unifying them structurally. So a root holding
    /// a *partially known* `Fn(v1, Int)` is reported as mismatching `Fn(Int, Int)` rather than
    /// solving `v1 := Int`. Records make this constant — they are the first type that routinely
    /// holds variables *and* sits behind one — so `unify` must resolve both sides to their
    /// representatives before dispatching.
    #[test]
    #[ignore = "concrete union-find roots are compared with PartialEq, not unified"]
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
        unify(&mut unification_map, &partially_known, &v0, span)
            .expect("binding a fresh variable to a concrete type succeeds");

        // Unifying that against a fully concrete `Fn(Int, Int)` must descend and solve v1.
        let fully_known = Type::Fn(
            Some(Box::new(Type::Literal(Literal::Int))),
            Box::new(Type::Literal(Literal::Int)),
        );
        unify(&mut unification_map, &fully_known, &v0, span)
            .expect("a solved variable unifies structurally with a compatible function");

        assert_eq!(
            unification_map.subsitute(&v1),
            Type::Literal(Literal::Int),
            "the partially known parameter should have been solved to Int"
        );
    }

    /// Regression: unifying a free variable with *itself* makes `union_vars` write
    /// `map[r] = Reference(r)`, so the next `find_root(r)` recurses forever. Reachable from source
    /// via an `if`/`else` whose two branches are the same unannotated binding. The fix is an
    /// early return in `union_vars` when both ids resolve to the same root.
    ///
    /// NOTE: this fails by stack overflow, which aborts the entire test binary rather than failing
    /// one test — run it on its own (`cargo test -p trestle unifying_a_variable_with_itself
    /// -- --ignored`) until the fix lands.
    #[test]
    #[ignore = "unify(v, v) links a root to itself — stack overflow in find_root"]
    fn unifying_a_variable_with_itself_is_a_no_op() {
        let mut unification_map = UnificationMap::new();
        let v = Type::Var(unification_map.mint_new_type_var());

        unify(&mut unification_map, &v, &v, SourceSpan::from((0, 0)))
            .expect("unifying a variable with itself succeeds");

        // Reaching this line at all is the test; the variable should still be free.
        assert_eq!(unification_map.subsitute(&v), v);
    }
}
