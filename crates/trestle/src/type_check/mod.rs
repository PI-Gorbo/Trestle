//! Type checking. Turns a [`BindingResolvedProgram`] into a [`TypeCheckedProgram`] by computing a
//! [`Type`](typed_ast::Type) for every node and interpreting annotations.
//!
//! The pass is split into cohesive submodules:
//! - [`typed_ast`] — the output IR (types + typed tree).
//! - [`unification`] — the union-find over type variables and the core `UnificationMap::unify`.
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

use binding_table::attach_types_to_bindings;
use inference::{InferenceCtx, infer_type_of_expression};
use substitution::subsitute_in_expr;
use typed_ast::TypeCheckedExpression;
use unification::UnificationMap;

struct TypeCheckState {
    inference_ctx: InferenceCtx,
    unification_map: UnificationMap,
    expressions: Vec<TypeCheckedExpression>,
    errors: Vec<TypeCheckError>,
}

/// Type-check a name-resolved program into a fully typed [`TypeCheckedProgram`].
pub fn type_check(
    program: BindingResolvedProgram,
) -> Result<TypeCheckedProgram, Vec<TypeCheckError>> {
    let BindingResolvedProgram {
        expressions,
        bindings,
        type_bindings,
    } = program;

    // Borrow `bindings` for id lookups during the walk; it's consumed afterwards (moving each
    // name across) to build the typed table.
    let expression_count = expressions.len();
    let final_state = expressions.into_iter().fold(
        TypeCheckState {
            expressions: Vec::with_capacity(expression_count),
            errors: Vec::new(),
            inference_ctx: InferenceCtx::new(bindings.len(), type_bindings.len()),
            unification_map: UnificationMap::new(),
        },
        |mut state, untyped_expression| {
            match infer_type_of_expression(
                untyped_expression,
                &mut state.inference_ctx,
                &mut state.unification_map,
                &bindings,
            ) {
                Ok(expression) => state.expressions.push(expression),
                Err(error) => state.errors.push(error),
            }

            state
        },
    );

    // Report inference's failures before finalising the binding table. An error raised inside a
    // value expression (a lambda body, say) leaves its enclosing binding untyped, so
    // `resolve_bindings` would fail too — and its `UntypedBindingAfterTypeCheck` is an
    // internal-consistency check, only meaningful once inference has *succeeded*. Zipping first
    // would mask the diagnostic the user actually needs with a compiler-bug report.
    if !final_state.errors.is_empty() {
        return Err(final_state.errors);
    }

    // Binding types are recorded during inference with their type variables intact (a `let`
    // without an annotation is bound to a fresh `Var`), so resolve them the same way the
    // expression tree is resolved below. Each namespace zips against its own env.
    let variable_bindings_with_types = attach_types_to_bindings(
        bindings,
        &final_state.inference_ctx.variable_env,
        &final_state.unification_map,
    )
    .map_err(|err| vec![err])?;

    let type_bindings_with_types = attach_types_to_bindings(
        type_bindings,
        &final_state.inference_ctx.type_env,
        &final_state.unification_map,
    )
    .map_err(|err| vec![err])?;

    let mut subsituted_expressions = final_state.expressions;
    subsituted_expressions
        .iter_mut()
        .for_each(|expr| subsitute_in_expr(&final_state.unification_map, expr));

    Ok(TypeCheckedProgram {
        expressions: subsituted_expressions,
        bindings: variable_bindings_with_types,
        type_bindings: type_bindings_with_types,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Drive the type-check pass directly: parse, resolve names, then type-check.
    fn analyse_src(src: &str) -> Result<TypeCheckedProgram, Vec<TypeCheckError>> {
        let parsed = crate::parse::parse(src).expect("test source should parse");
        let resolved =
            crate::binding_resolution::resolve(parsed).expect("test source should resolve");

        type_check(resolved)
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
        let analysis = analyse_src("let f = (a: Int) => a\nf(1, 2)");
        let error = analysis.expect_err("over-application is an error");
        assert!(matches!(error[0], TypeCheckError::TooManyArguments { .. }));
    }
}
