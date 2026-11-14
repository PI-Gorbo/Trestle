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
use inference::InferenceCtx;
use substitution::substitute_in_expr;
use typed_ast::TypeCheckedExpression;

struct TypeCheckState<'a> {
    inference_ctx: InferenceCtx<'a>,
    expressions: Vec<TypeCheckedExpression>,
    errors: Vec<TypeCheckError>,
}

pub fn type_check(
    program: BindingResolvedProgram,
) -> Result<TypeCheckedProgram, Vec<TypeCheckError>> {
    let BindingResolvedProgram {
        expressions,
        bindings,
        type_bindings,
    } = program;

    // Part 1, fold over all expressions, infer their type and collect all errors.
    let expression_count = expressions.len();
    let final_state = expressions.into_iter().fold(
        TypeCheckState {
            expressions: Vec::with_capacity(expression_count),
            errors: Vec::new(),
            inference_ctx: InferenceCtx::new(&bindings, type_bindings.len()),
        },
        |mut state, untyped_expression| {
            match state.inference_ctx.infer(untyped_expression) {
                Ok(expression) => state.expressions.push(expression),
                Err(error) => state.errors.push(error),
            }

            state
        },
    );

    let TypeCheckState {
        inference_ctx,
        expressions,
        errors,
    } = final_state;

    if !errors.is_empty() {
        return Err(errors);
    }

    // Map the result of the inference to a type checked program.
    let InferenceCtx {
        variable_env,
        type_env,
        unification_map,
        ..
    } = inference_ctx;

    let variable_bindings_with_types =
        attach_types_to_bindings(bindings, &variable_env, &unification_map)
            .map_err(|err| vec![err])?;

    let type_bindings_with_types =
        attach_types_to_bindings(type_bindings, &type_env, &unification_map)
            .map_err(|err| vec![err])?;

    let substituted_expressions = expressions
        .into_iter()
        .map(|mut expr| {
            substitute_in_expr(&unification_map, &mut expr);
            expr
        })
        .collect();

    Ok(TypeCheckedProgram {
        expressions: substituted_expressions,
        bindings: variable_bindings_with_types,
        type_bindings: type_bindings_with_types,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::parse::ast::{ExpressionKind, Literal, ParsedProgram};

    fn analyse_src(src: &str) -> Result<TypeCheckedProgram, Vec<TypeCheckError>> {
        analyse_parsed(crate::parse::parse(src).expect("test source should parse"))
    }

    fn analyse_parsed(parsed: ParsedProgram) -> Result<TypeCheckedProgram, Vec<TypeCheckError>> {
        let resolved =
            crate::binding_resolution::resolve(parsed).expect("test source should resolve");

        type_check(resolved)
    }

    #[test]
    fn let_annotation_mismatch_is_an_error() {
        let errors = analyse_src("let x: Int = \"hello\"")
            .expect_err("String value annotated Int is a type error");
        assert!(matches!(errors[0], TypeCheckError::TypeMismatch { .. }));
    }

    #[test]
    fn accessing_an_absent_field_is_an_error() {
        let errors =
            analyse_src("type Point = { x: Int, y: Int }\nlet p: Point = { x: 1, y: 2 }\np.z")
                .expect_err("a field the record lacks is a type error");
        let TypeCheckError::RecordDoesNotHaveField {
            field_name,
            available,
            ..
        } = &errors[0]
        else {
            panic!("expected RecordDoesNotHaveField, got {:?}", errors[0]);
        };
        assert_eq!(field_name, "z");
        assert_eq!(available, &["x".to_string(), "y".to_string()]);
    }

    #[test]
    fn accessing_a_field_on_a_non_record_is_an_error() {
        let errors = analyse_src("let n: Int = 1\nn.x")
            .expect_err("field access on a non-record is a type error");
        assert!(matches!(errors[0], TypeCheckError::NotARecord { .. }));
    }

    #[test]
    fn too_many_arguments_is_an_error() {
        const SRC: &str = "let f = (a: Int) => a\nf(1, 2)";

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
