//! Walkers for `Rule::expr` and everything nested under it: lambdas, type
//! declarations, and binary operators.
//!
//! Binary-operator precedence is owned by a pest [`PrattParser`] (`PRATT`), not
//! the grammar: the flat `binary_expression` rule hands its `primary`/operator
//! sequence to it. Every builder returns a fully source-spanned [`Expression`];
//! synthesized `Binary` nodes span both operands via [`merge_spans`].

use std::sync::LazyLock;

use pest::pratt_parser::{Assoc, Op, PrattParser};
use pest::{Span, iterators::Pair};

use crate::parse::ast::{BinaryOp, Literal};

use super::{BuildError, Rule};

use super::ast::{
    Expression, ExpressionKind, Lambda, Param, TypeDeclaration, get_bindings, merge_spans,
    source_span_from_pest_span,
};

/// The operator-precedence table — the single, explicit statement of Trestle's
/// order of operations. Levels are listed loosest-binding first, so:
///   comparison  <  additive  <  multiplicative
/// Every operator is left-associative (pest offers only `Left`/`Right`, so a
/// chain like `a < b < c` parses as `(a < b) < c` and later type-errors).
static PRATT: LazyLock<PrattParser<Rule>> = LazyLock::new(|| {
    PrattParser::new()
        .op(Op::infix(Rule::eq, Assoc::Left)
            | Op::infix(Rule::neq, Assoc::Left)
            | Op::infix(Rule::lt, Assoc::Left)
            | Op::infix(Rule::gt, Assoc::Left)
            | Op::infix(Rule::le, Assoc::Left)
            | Op::infix(Rule::ge, Assoc::Left))
        .op(Op::infix(Rule::add, Assoc::Left) | Op::infix(Rule::subtract, Assoc::Left))
        .op(Op::infix(Rule::multiply, Assoc::Left) | Op::infix(Rule::divide, Assoc::Left))
});

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
        Rule::list_of_expressions => build_list_of_expressions(expr_binding),
        Rule::let_expression => build_let(expr_binding),
        Rule::lambda_expression => build_lambda(expr_binding),
        Rule::binary_expression => build_binary(expr_binding),
        rule => Err(BuildError::UnexpectedRule {
            rule,
            span: source_span_from_pest_span(expr_binding.as_span()),
        }),
    }
}

/// Build a `Block` from a `Rule::list_of_expressions` pair: a brace-wrapped sequence
/// of expressions whose value is its last element.
fn build_list_of_expressions(pair: Pair<Rule>) -> Result<Expression, BuildError> {
    let span = pair.as_span();
    let expressions = pair.into_inner().try_fold(Vec::new(), |mut list, expr| {
        list.push(build_expr(expr)?);
        Ok(list)
    })?;
    Ok(spanned(span, ExpressionKind::Block(expressions)))
}

/// Build a `Let` from a `Rule::let_binding` pair.
fn build_let(pair: Pair<Rule>) -> Result<Expression, BuildError> {
    let span = pair.as_span();

    let (name, value) =
        pair.into_inner()
            .fold((String::new(), None), |(mut name, mut value), p| {
                match p.as_rule() {
                    Rule::let_kw => {}
                    Rule::identifier_with_optional_type_declaration => {
                        let ident = p
                            .into_inner()
                            .next()
                            .expect("binding target starts with an identifier");
                        name = ident.as_str().to_string();
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

    // Guard: a lambda must have a body.
    let Some(body_value) = body else {
        return Err(BuildError::MissingLambdaBody {
            span: source_span_from_pest_span(span),
        });
    };
    let boxed_body = Box::new(body_value?);

    // Fold up the params to build a curried lambda expression. Ie: (A => (B => (C => D)))
    let mut params_in_reverse = params.into_iter().rev();

    // Guard: a lambda with no parameters wraps the body directly.
    let Some(last_param) = params_in_reverse.next() else {
        return Ok(spanned(
            span,
            ExpressionKind::Lambda(Lambda {
                parameter: None,
                return_type,
                body: boxed_body,
            }),
        ));
    };

    // The innermost lambda owns the real return type; outer wrappers get None.
    let most_inner_lambda = Lambda {
        parameter: Some(last_param),
        body: boxed_body,
        return_type,
    };

    Ok(spanned(
        span,
        ExpressionKind::Lambda(params_in_reverse.fold(
            most_inner_lambda,
            |inner_lambda, next_innermost_parameter| Lambda {
                parameter: Some(next_innermost_parameter),
                return_type: None,
                body: Box::new(spanned(span, ExpressionKind::Lambda(inner_lambda))),
            },
        )),
    ))
}

struct BuildParamCtx {
    name: Option<String>,
    type_dec: Option<TypeDeclaration>,
}

fn build_param(pair: Pair<Rule>) -> Result<Param, BuildError> {
    let span = source_span_from_pest_span(pair.as_span());

    pair.into_inner()
        .try_fold(
            BuildParamCtx {
                name: None,
                type_dec: None,
            },
            |state, pair| match pair.as_rule() {
                Rule::identifier => Ok(BuildParamCtx {
                    name: Some(pair.as_str().to_string()),
                    ..state
                }),
                Rule::type_declaration => Ok(BuildParamCtx {
                    type_dec: Some(build_type(pair)),
                    ..state
                }),
                rule => Err(BuildError::UnexpectedRule { rule, span }),
            },
        )
        // The grammar accepts untyped params (so `=>` commits the lambda branch); a
        // required type that's missing is rejected here, pointing the caret at the param.
        .and_then(|values| match values {
            BuildParamCtx {
                name: Some(name),
                type_dec: Some(type_dec),
            } => Ok(Param { name, type_dec }),
            BuildParamCtx {
                name: Some(name),
                type_dec: None,
            } => Err(BuildError::MissingParamType { name, span }),
            _ => Err(BuildError::Invariant { span }),
        })
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

/// Fold a `Rule::binary_expression` (`primary (op primary)*`) into a tree of
/// [`ExpressionKind::Binary`] nodes using the [`PRATT`] precedence table. A lone
/// primary passes straight through — no `Binary` wrapper.
fn build_binary(pair: Pair<Rule>) -> Result<Expression, BuildError> {
    PRATT
        .map_primary(build_primary)
        .map_infix(|lhs, op, rhs| {
            let lhs = lhs?;
            let rhs = rhs?;
            let span = merge_spans(lhs.span, rhs.span);
            let binary_op = match op.as_rule() {
                Rule::add => BinaryOp::Add,
                Rule::subtract => BinaryOp::Sub,
                Rule::multiply => BinaryOp::Mul,
                Rule::divide => BinaryOp::Div,
                Rule::lt => BinaryOp::Lt,
                Rule::gt => BinaryOp::Gt,
                Rule::le => BinaryOp::Le,
                Rule::ge => BinaryOp::Ge,
                Rule::eq => BinaryOp::Eq,
                Rule::neq => BinaryOp::Neq,
                rule => {
                    return Err(BuildError::UnexpectedRule {
                        rule,
                        span: source_span_from_pest_span(op.as_span()),
                    });
                }
            };
            Ok(Expression {
                kind: ExpressionKind::Binary(binary_op, Box::new(lhs), Box::new(rhs)),
                span,
            })
        })
        .parse(pair.into_inner())
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
        ExpressionKind::FunctionInvocation {
            function_name: identifier,
            expressions: parameters,
        },
    ))
}

fn build_literal(pair: Pair<Rule>) -> Result<Expression, BuildError> {
    let child = pair.into_inner().next().expect("literal has one child");
    let span = child.as_span();
    match child.as_rule() {
        Rule::int => Ok(spanned(
            span,
            ExpressionKind::Literal(Literal::Int(
                child.as_str().parse().expect("int literal fits in usize"),
            )),
        )),
        Rule::string => Ok(spanned(
            span,
            ExpressionKind::Literal(Literal::String(child.as_str().to_string())),
        )),
        Rule::boolean => Ok(spanned(
            span,
            ExpressionKind::Literal(Literal::Bool(child.as_str() == "true")),
        )),
        Rule::float => Ok(spanned(
            span,
            ExpressionKind::Literal(Literal::Float(
                child.as_str().parse().expect("float literal parses as f64"),
            )),
        )),
        rule => Err(BuildError::UnexpectedRule {
            rule,
            span: source_span_from_pest_span(span),
        }),
    }
}

fn build_primary(pair: Pair<Rule>) -> Result<Expression, BuildError> {
    let child = pair.into_inner().next().expect("primary has one child");
    let span = child.as_span();
    match child.as_rule() {
        Rule::function_invocation => build_function_invocation(child),
        Rule::literal => build_literal(child),
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

#[cfg(test)]
mod tests {
    use crate::parse::parse;

    /// A lambda whose param lacks a type must fail with a *targeted* message pointing at
    /// the param — not the old raw-pest "expected EOI/operator" error at `=>`. The grammar
    /// admits untyped params so the `=>` commits the lambda branch; the missing type is
    /// then rejected in `build_param`.
    #[test]
    fn untyped_lambda_param_reports_missing_type() {
        let report = parse("(n) => n").expect_err("untyped param must be rejected");
        let rendered = format!("{report:?}");
        assert!(
            rendered.contains("requires a type annotation"),
            "expected a missing-type diagnostic, got:\n{rendered}"
        );
    }
}
