//! Multi-parameter function types and lambdas are curried during parsing. These tests
//! flatten the curried chain back out and check the parameters come out in source order.

use trestle::parse::ast::{ExpressionKind, TypeExpression, TypeExpressionKind};

fn single_expression(src: &str) -> ExpressionKind {
    let mut program = trestle::parse(src).expect("program should parse");
    assert_eq!(program.expressions.len(), 1);
    program.expressions.remove(0).kind
}

/// `Named` types print as their name; anything else as `_`. Good enough to tell params apart.
fn type_name(type_expression: &TypeExpression) -> String {
    match &type_expression.kind {
        TypeExpressionKind::Named(name) => name.clone(),
        _ => "_".to_string(),
    }
}

/// Flattens `(a: A) => (b: B) => R` into `([("a", "A"), ("b", "B")], "R")`.
fn flatten_type(mut type_expression: &TypeExpression) -> (Vec<(String, String)>, String) {
    let mut params = Vec::new();
    while let TypeExpressionKind::Lambda(lambda) = &type_expression.kind {
        if let Some(param) = &lambda.parameter {
            params.push((
                param.name.clone().unwrap_or_default(),
                type_name(&param.type_dec),
            ));
        }
        type_expression = &lambda.return_type;
    }
    (params, type_name(type_expression))
}

fn declared_type(src: &str) -> (Vec<(String, String)>, String) {
    match single_expression(src) {
        ExpressionKind::TypeDeclaration {
            type_expression, ..
        } => flatten_type(&type_expression),
        other => panic!("expected a type declaration, got {other:?}"),
    }
}

fn pairs(expected: &[(&str, &str)]) -> Vec<(String, String)> {
    expected
        .iter()
        .map(|(n, t)| (n.to_string(), t.to_string()))
        .collect()
}

#[test]
fn function_type_parameters_keep_source_order() {
    let (params, ret) = declared_type("type F = (a: Int, b: String, c: Bool) => Float");
    assert_eq!(
        params,
        pairs(&[("a", "Int"), ("b", "String"), ("c", "Bool")])
    );
    assert_eq!(ret, "Float");
}

#[test]
fn unnamed_function_type_parameters_keep_source_order() {
    let (params, ret) = declared_type("type F = (Int, String) => Bool");
    assert_eq!(params, pairs(&[("", "Int"), ("", "String")]));
    assert_eq!(ret, "Bool");
}

#[test]
fn nested_function_type_parameter_keeps_its_position() {
    let (params, ret) = declared_type("type F = (f: (Int) => String, x: Int) => String");
    assert_eq!(params, pairs(&[("f", "_"), ("x", "Int")]));
    assert_eq!(ret, "String");
}

#[test]
fn lambda_parameters_keep_source_order() {
    let mut kind = single_expression("(a: Int, b: String, c) => a");
    let mut params = Vec::new();
    while let ExpressionKind::Lambda(lambda) = kind {
        if let Some(param) = lambda.parameter {
            params.push((
                param.name,
                param
                    .type_dec
                    .as_ref()
                    .map_or("", |t| match &t.kind {
                        TypeExpressionKind::Named(name) => name,
                        _ => "_",
                    })
                    .to_string(),
            ));
        }
        kind = lambda.body.kind;
    }
    assert_eq!(params, pairs(&[("a", "Int"), ("b", "String"), ("c", "")]));
    assert!(matches!(kind, ExpressionKind::Var(ref v) if v == "a"));
}
