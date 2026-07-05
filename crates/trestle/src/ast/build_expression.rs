//! Walkers for `Rule::expr` and everything nested under it: lambdas, type
//! declarations, and the precedence-encoded arithmetic levels.
//!
//! Precedence is encoded by the grammar's rule nesting (`add` < `mul`), so
//! each level only has to left-fold its children. Every builder returns a
//! fully source-spanned [`Expression`]; synthesized binary nodes span both
//! operands via [`merge_spans`].

use pest::{Span, iterators::Pair};

use crate::{Rule, ast::BuildError};

use super::{
    Expression, ExpressionKind, Lambda, Param, TypeDeclaration, get_bindings, merge_spans,
    source_span_from_pest_span,
};

/// Wrap an [`ExpressionKind`] with the source span of the pest pair it came from.
fn spanned(span: Span, kind: ExpressionKind) -> Expression {
    Expression {
        kind,
        span: source_span_from_pest_span(span),
    }
}

pub fn build_expr(pair: Pair<Rule>) -> Result<Expression, BuildError> {
    let expr_binding = get_bindings(pair, "expression to have bindings");
    match expr_binding.as_rule() {
        Rule::let_binding => build_let(expr_binding),
        Rule::lambda => build_lambda(expr_binding),
        Rule::add => build_add(expr_binding),
        rule => Err(BuildError::UnexpectedRule {
            rule,
            span: source_span_from_pest_span(expr_binding.as_span()),
        }),
    }
}

/// Build a `Let` from a `Rule::let_binding` pair.
fn build_let(pair: Pair<Rule>) -> Result<Expression, BuildError> {
    let span = pair.as_span();

    let (name, value) =
        pair
            .into_inner()
            .fold((String::new(), None), |(mut name, mut value), p| {
                match p.as_rule() {
                    Rule::let_kw => {}
                    Rule::identifier_with_optional_type_declaration => {
                        name = p.as_str().to_string()
                    }
                    Rule::expr => value = Some(build_expr(p)),
                    rule => unreachable!("unexpected rule in let_binding: {:?}", rule),
                }
                (name, value)
            });

    match value {
        Some(expr) => Ok(spanned(
            span,
            ExpressionKind::Let {
                name,
                value: Box::new(expr?),
            },
        )),
        None => Err(BuildError::MissingLetBody {
            span: source_span_from_pest_span(span),
        }),
    }
}

fn build_lambda(pair: Pair<Rule>) -> Result<Expression, BuildError> {
    let span = pair.as_span();
    let mut params = Vec::new();
    let mut return_type = None;
    let mut body = None;

    for p in pair.into_inner() {
        match p.as_rule() {
            Rule::identifier_with_optional_type_declaration => params.push(build_param(p)?),
            Rule::optional_type_declaration => return_type = build_type_opt(p),
            Rule::expr => body = Some(build_expr(p)),
            rule => {
                return Err(BuildError::UnexpectedRule {
                    rule,
                    span: source_span_from_pest_span(p.as_span()),
                });
            }
        }
    }

    match body {
        Some(body_value) => Ok(spanned(
            span,
            ExpressionKind::Lambda(Lambda {
                params,
                return_type,
                body: Box::new(body_value?),
            }),
        )),
        None => Err(BuildError::MissingLambdaBody {
            span: source_span_from_pest_span(span),
        }),
    }
}

fn build_param(pair: Pair<Rule>) -> Result<Param, BuildError> {
    let mut name = String::new();
    let mut ty = None;
    for p in pair.into_inner() {
        match p.as_rule() {
            Rule::identifier => name = p.as_str().to_string(),
            Rule::type_declaration => ty = Some(build_type(p)),
            rule => {
                return Err(BuildError::UnexpectedRule {
                    rule,
                    span: source_span_from_pest_span(p.as_span()),
                });
            }
        }
    }
    Ok(Param { name, type_dec: ty })
}

fn build_type_opt(pair: Pair<Rule>) -> Option<TypeDeclaration> {
    pair.into_inner().next().map(build_type)
}

fn build_type(pair: Pair<Rule>) -> TypeDeclaration {
    let ident = pair
        .into_inner()
        .next()
        .expect("type_declaration has an identifier");
    TypeDeclaration::Named(ident.as_str().to_string())
}

fn build_add(pair: Pair<Rule>) -> Result<Expression, BuildError> {
    let mut inner = pair.into_inner();
    let head = inner.next().expect("add starts with a #head mul");
    let head_expression = build_mul(head)?;
    inner.try_fold(head_expression, |lhs, next| {
        let rhs = build_mul(next)?;
        let span = merge_spans(lhs.span, rhs.span);

        Ok(Expression {
            kind: ExpressionKind::Add(Box::new(lhs), Box::new(rhs)),
            span,
        })
    })
}

fn build_mul(pair: Pair<Rule>) -> Result<Expression, BuildError> {
    let mut inner = pair.into_inner();
    let head = inner.next().expect("mul has at least one primary");
    let head_expression = build_primary(head)?;
    inner.try_fold(head_expression, |lhs, next| {
        let rhs = build_primary(next)?;
        let span = merge_spans(lhs.span, rhs.span);

        Ok(Expression {
            kind: ExpressionKind::Mul(Box::new(lhs), Box::new(rhs)),
            span,
        })
    })
}

fn build_comma_separated_list_of_expressions(
    pair: Pair<Rule>,
) -> Result<Vec<Expression>, BuildError> {
    let mut inner = pair.into_inner();
    inner.try_fold(Vec::new(), |mut list, expression| {
        let expression = build_expr(expression)?;
        list.push(expression);

        Ok(list)
    })
}

fn build_function_invocation(pair: Pair<Rule>) -> Result<Expression, BuildError> {
    let span = pair.as_span();
    let inner = pair.into_inner();
    let mut identifier = String::new();
    let mut parameters = Vec::new();

    for p in inner {
        match p.as_rule() {
            Rule::identifier => identifier = p.as_str().to_string(),
            Rule::comma_separated_list_of_expressions => {
                parameters = build_comma_separated_list_of_expressions(p)?;
            }
            rule => {
                return Err(BuildError::UnexpectedRule {
                    rule,
                    span: source_span_from_pest_span(p.as_span()),
                });
            }
        }
    }

    Ok(spanned(
        span,
        ExpressionKind::FunctionInvocation(identifier, parameters),
    ))
}

fn build_primary(pair: Pair<Rule>) -> Result<Expression, BuildError> {
    let child = pair.into_inner().next().expect("primary has one child");
    let span = child.as_span();
    match child.as_rule() {
        Rule::function_invocation => build_function_invocation(child),
        Rule::int => Ok(spanned(
            span,
            ExpressionKind::Int(child.as_str().parse().expect("int literal fits in i64")),
        )),
        Rule::identifier => Ok(spanned(
            span,
            ExpressionKind::Var(child.as_str().to_string()),
        )),
        Rule::expr => build_expr(child), // parenthesized expression
        rule => Err(BuildError::UnexpectedRule {
            rule,
            span: source_span_from_pest_span(span),
        }),
    }
}
