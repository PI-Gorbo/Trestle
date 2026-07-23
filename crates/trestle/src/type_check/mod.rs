//! Type checking. Turns a [`BindingResolvedProgram`] into a [`TypeCheckedProgram`] by computing a
//! [`Type`](typed_ast::Type) for every node and interpreting annotations.
//!
//! The pass is split into cohesive submodules:
//! - [`typed_ast`] — the output IR (types + typed tree).
//! - [`unification`] — the union-find over type variables and the core [`unify`](unification::unify).
//! - [`inference`] — the bottom-up walk that synthesises a type per node.
//! - [`binding_table`] — the per-binding type table and its finalisation.
//! - [`substitution`] — the final pass that resolves every solved variable in the tree.
//! - [`error`] — [`TypeCheckError`].

mod binding_table;
mod error;
mod inference;
mod substitution;
pub mod typed_ast;
mod unification;

pub use error::TypeCheckError;
pub use typed_ast::TypeCheckedProgram;

use crate::binding_resolution::BindingResolvedProgram;

use binding_table::{BindingToTypeMap, zip_bindings_with_types};
use inference::infer_type_of_expression;
use substitution::subsitute;
use typed_ast::TypeCheckedExpression;
use unification::UnificationMap;

struct TypeCheckState {
    binding_type_map: BindingToTypeMap,
    unification_map: UnificationMap,
    expressions: Vec<TypeCheckedExpression>,
    errors: Vec<TypeCheckError>,
}

/// Type-check a name-resolved program into a fully typed [`TypeCheckedProgram`].
pub fn analyse(
    program: BindingResolvedProgram,
) -> Result<TypeCheckedProgram, Vec<TypeCheckError>> {
    let BindingResolvedProgram {
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
        true => Ok(TypeCheckedProgram {
            expressions: subsituted_expressions,
            bindings: typed_bindings,
        }),
        false => Err(final_state.errors),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Drive the type-check pass directly: parse, resolve names, then type-check.
    fn analyse_src(src: &str) -> Result<TypeCheckedProgram, Vec<TypeCheckError>> {
        let parsed = crate::parse::parse(src).expect("test source should parse");
        let resolved =
            crate::binding_resolution::resolve(parsed).expect("test source should resolve");
        analyse(resolved)
    }

    #[test]
    fn let_annotation_mismatch_is_an_error() {
        // Annotating a String value as `Int` must be a type error.
        let errors = analyse_src("let x: Int = \"hello\"")
            .expect_err("String value annotated Int is a type error");
        assert!(matches!(errors[0], TypeCheckError::TypeMismatch { .. }));
    }

    #[test]
    fn too_many_arguments_is_an_error() {
        // `f` takes one argument; applying two over-applies it.
        let errors = analyse_src("let f = (a: Int) => a\nf(1, 2)")
            .expect_err("over-application is an error");
        assert!(matches!(errors[0], TypeCheckError::TooManyArguments { .. }));
    }
}
