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

    if !final_state.errors.is_empty() {
        return Err(final_state.errors);
    }

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

    use crate::parse::ast::{ExpressionKind, Literal, ParsedProgram};

    /// Drive the type-check pass directly: parse, resolve names, then type-check.
    fn analyse_src(src: &str) -> Result<TypeCheckedProgram, Vec<TypeCheckError>> {
        analyse_parsed(crate::parse::parse(src).expect("test source should parse"))
    }

    /// Resolve and type-check an already-parsed program, so a test can assert on the AST first.
    fn analyse_parsed(parsed: ParsedProgram) -> Result<TypeCheckedProgram, Vec<TypeCheckError>> {
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
        const SRC: &str = "let f = (a: Int) => a\nf(1, 2)";

        // The type error hinges on the parse: newlines are insignificant (trestle.pest),
        // so the two statements are delimited structurally. Pin that shape here — otherwise
        // a parser that drops the call postfix shows up as a confusing `expect_err` panic.
        let parsed = crate::parse::parse(SRC).expect("test source should parse");
        assert_eq!(
            parsed.expressions.len(),
            2,
            "expected a `let` and a call, got {:?}",
            parsed.expressions
        );

        match &parsed.expressions[0].kind {
            ExpressionKind::Let { name, value, .. } => {
                assert_eq!(name, "f");
                assert!(
                    matches!(value.kind, ExpressionKind::Lambda(_)),
                    "expected `f` to bind a lambda, got {:?}",
                    value.kind
                );
            }
            other => panic!("expected a `let` binding, got {other:?}"),
        }

        match &parsed.expressions[1].kind {
            ExpressionKind::FunctionInvocation {
                function,
                arguments,
            } => {
                assert!(
                    matches!(&function.kind, ExpressionKind::Var(name) if name == "f"),
                    "expected a call to `f`, got {:?}",
                    function.kind
                );
                assert_eq!(
                    arguments.len(),
                    2,
                    "expected `f(1, 2)` to carry both arguments"
                );
                assert!(matches!(
                    arguments[0].kind,
                    ExpressionKind::Literal(Literal::Int(1))
                ));
                assert!(matches!(
                    arguments[1].kind,
                    ExpressionKind::Literal(Literal::Int(2))
                ));
            }
            other => panic!("expected `f(1, 2)` to parse as a call, got {other:?}"),
        }

        let errors = analyse_parsed(parsed).expect_err("over-application is an error");
        assert!(matches!(errors[0], TypeCheckError::TooManyArguments { .. }));
    }
}
