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

use crate::parse::ast::{BinaryOp, Literal, UnaryOp};

use super::{BuildError, Rule};

use super::ast::{
    Expression, ExpressionKind, Lambda, Param, TypeDeclaration, get_bindings, merge_spans,
    source_span_from_pest_span,
};

/// The operator-precedence table — the single, explicit statement of Trestle's
/// order of operations. Levels are listed loosest-binding first, so:
///   pipe  <  or  <  and  <  comparison  <  additive  <  multiplicative  <  prefix
/// `|>` binds loosest so `a + b |> f` is `(a + b) |> f` and a chain
/// `x |> f |> g` is `(x |> f) |> g == g(f(x))`.
/// Every infix operator is left-associative (pest offers only `Left`/`Right`, so a
/// chain like `a < b < c` parses as `(a < b) < c` and later type-errors). Prefix ops
/// bind tightest, so `!a && b` is `(!a) && b` and `-a * b` is `(-a) * b`.
static PRATT: LazyLock<PrattParser<Rule>> = LazyLock::new(|| {
    PrattParser::new()
        .op(Op::infix(Rule::pipe, Assoc::Left))
        .op(Op::infix(Rule::or, Assoc::Left))
        .op(Op::infix(Rule::and, Assoc::Left))
        .op(Op::infix(Rule::eq, Assoc::Left)
            | Op::infix(Rule::neq, Assoc::Left)
            | Op::infix(Rule::lt, Assoc::Left)
            | Op::infix(Rule::gt, Assoc::Left)
            | Op::infix(Rule::le, Assoc::Left)
            | Op::infix(Rule::ge, Assoc::Left))
        .op(Op::infix(Rule::add, Assoc::Left) | Op::infix(Rule::subtract, Assoc::Left))
        .op(Op::infix(Rule::multiply, Assoc::Left) | Op::infix(Rule::divide, Assoc::Left))
        .op(Op::prefix(Rule::negate) | Op::prefix(Rule::logical_not))
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
        Rule::if_expression => build_if_expression(expr_binding),
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

    let (name, type_dec, value) = pair.into_inner().fold(
        (String::new(), None, None),
        |(mut name, mut type_dec, mut value), p| {
            match p.as_rule() {
                Rule::let_kw => {}
                Rule::identifier_with_optional_type_declaration => {
                    (name, type_dec) = build_binding_target(p);
                }
                Rule::expr => value = Some(build_expr(p)),
                rule => unreachable!("unexpected rule in let_binding: {:?}", rule),
            }
            (name, type_dec, value)
        },
    );

    match value {
        Some(expr) => Ok(spanned(
            span,
            ExpressionKind::Let {
                name,
                type_dec,
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

/// Parse an `identifier_with_optional_type_declaration` (`identifier ~
/// (type_declaration)?`) into its binding name and optional type annotation.
/// Shared by `let` bindings (annotation optional) and `build_param` (annotation
/// required — the caller enforces it). The grammar guarantees the identifier.
fn build_binding_target(pair: Pair<Rule>) -> (String, Option<TypeDeclaration>) {
    pair.into_inner()
        .fold((String::new(), None), |(mut name, mut type_dec), p| {
            match p.as_rule() {
                Rule::identifier => name = p.as_str().to_string(),
                Rule::type_declaration => type_dec = Some(build_type(p)),
                rule => unreachable!(
                    "unexpected rule in identifier_with_optional_type_declaration: {:?}",
                    rule
                ),
            }
            (name, type_dec)
        })
}

fn build_param(pair: Pair<Rule>) -> Result<Param, BuildError> {
    // The grammar accepts untyped params (so `=>` commits the lambda branch); a
    // required type that's missing is rejected here, pointing the caret at the param.
    let (name, type_dec) = build_binding_target(pair);

    Ok(Param { name, type_dec })
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
                Rule::and => BinaryOp::And,
                Rule::or => BinaryOp::Or,
                Rule::lt => BinaryOp::Lt,
                Rule::gt => BinaryOp::Gt,
                Rule::le => BinaryOp::Le,
                Rule::ge => BinaryOp::Ge,
                Rule::eq => BinaryOp::Eq,
                Rule::neq => BinaryOp::Neq,
                Rule::pipe => BinaryOp::Pipe,
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
        .map_prefix(|op, rhs| {
            let rhs = rhs?;
            // Span runs from the operator token through the operand: `merge_spans`
            // assumes the first span starts at or before the second, which holds
            // here since the prefix operator precedes its operand.
            let span = merge_spans(source_span_from_pest_span(op.as_span()), rhs.span);
            let unary_op = match op.as_rule() {
                Rule::negate => UnaryOp::Neg,
                Rule::logical_not => UnaryOp::Not,
                rule => {
                    return Err(BuildError::UnexpectedRule {
                        rule,
                        span: source_span_from_pest_span(op.as_span()),
                    });
                }
            };
            Ok(Expression {
                kind: ExpressionKind::Unary(unary_op, Box::new(rhs)),
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
        Rule::string => {
            // `as_str()` is the raw token incl. the surrounding quotes; strip them,
            // then resolve escape sequences to their runtime characters.
            let raw = child.as_str();
            let inner = &raw[1..raw.len() - 1]; // quotes are single-byte ASCII
            let value =
                unescaper::unescape(inner).map_err(|err| BuildError::InvalidStringEscape {
                    message: err.to_string(),
                    span: source_span_from_pest_span(span),
                })?;
            Ok(spanned(
                span,
                ExpressionKind::Literal(Literal::String(value)),
            ))
        }
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
        Rule::unit => Ok(spanned(span, ExpressionKind::Literal(Literal::Unit))),
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

/// Build an `If` from a `Rule::if_expression` pair. The grammar's keywords (`if`,
/// `else`) and delimiters are bare string literals, so `into_inner()` yields only the
/// `expr` children in order: condition, then-branch, and an optional else-branch.
fn build_if_expression(pair: Pair<Rule>) -> Result<Expression, BuildError> {
    let span = pair.as_span();
    let mut inner = pair.into_inner();

    let condition = build_expr(inner.next().expect("if_expression has a condition"))?;
    let true_pathway = build_expr(inner.next().expect("if_expression has a then branch"))?;
    let false_pathway = inner.next().map(build_expr).transpose()?; // None when no `else`

    Ok(spanned(
        span,
        ExpressionKind::If {
            condition: Box::new(condition),
            true_pathway: Box::new(true_pathway),
            false_pathway: false_pathway.map(Box::new),
        },
    ))
}

#[cfg(test)]
mod tests {
    use crate::parse::parse;

    /// A string literal's escape sequences are resolved to their runtime characters:
    /// the source `"a\nb"` stores the three-character value `a<newline>b`, and the
    /// surrounding quotes are stripped.
    #[test]
    fn string_literal_escapes_are_unescaped() {
        match only_expr_kind(r#""a\nb""#) {
            ExpressionKind::Literal(Literal::String(s)) => assert_eq!(s, "a\nb"),
            other => panic!("expected string literal, got {other:?}"),
        }
    }

    /// An invalid escape sequence is rejected with a targeted diagnostic rather than
    /// silently mangling the value or panicking.
    #[test]
    fn invalid_string_escape_reports_diagnostic() {
        let report = parse(r#""a\xZZ""#).expect_err("invalid escape must be rejected");
        let rendered = format!("{report:?}");
        assert!(
            rendered.contains("invalid escape sequence"),
            "expected an invalid-escape diagnostic, got:\n{rendered}"
        );
    }

    use crate::parse::ast::{BinaryOp, ExpressionKind, Literal, UnaryOp};

    /// Pull the single top-level expression's kind out of a parsed program.
    fn only_expr_kind(source: &str) -> ExpressionKind {
        let program = parse(source).expect("source parses");
        let mut expressions = program.expressions.into_iter();
        let expr = expressions.next().expect("one top-level expression");
        assert!(expressions.next().is_none(), "expected a single expression");
        expr.kind
    }

    /// `if (cond) then else other` maps the three exprs positionally: condition,
    /// then-branch, else-branch.
    #[test]
    fn if_with_else_maps_all_three_branches_positionally() {
        match only_expr_kind("if (x) 1 else 2") {
            ExpressionKind::If {
                condition,
                true_pathway,
                false_pathway,
            } => {
                assert!(matches!(condition.kind, ExpressionKind::Var(ref v) if v == "x"));
                assert!(matches!(
                    true_pathway.kind,
                    ExpressionKind::Literal(Literal::Int(1))
                ));
                let else_expr = false_pathway.expect("else branch present");
                assert!(matches!(
                    else_expr.kind,
                    ExpressionKind::Literal(Literal::Int(2))
                ));
            }
            other => panic!("expected If, got {other:?}"),
        }
    }

    /// The trailing `else` is optional; without it `false_pathway` is `None`.
    #[test]
    fn if_without_else_has_no_else_branch() {
        match only_expr_kind("if (x) 1") {
            ExpressionKind::If { false_pathway, .. } => {
                assert!(false_pathway.is_none(), "expected no else branch");
            }
            other => panic!("expected If, got {other:?}"),
        }
    }

    /// A prefix operator binds tighter than a binary one: `!a && b` is `(!a) && b`,
    /// so the `Not` wraps only `a` and the whole thing is an `And`.
    #[test]
    fn logical_not_binds_tighter_than_and() {
        match only_expr_kind("!a && b") {
            ExpressionKind::Binary(BinaryOp::And, lhs, rhs) => {
                assert!(matches!(lhs.kind, ExpressionKind::Unary(UnaryOp::Not, _)));
                assert!(matches!(rhs.kind, ExpressionKind::Var(ref v) if v == "b"));
            }
            other => panic!("expected And, got {other:?}"),
        }
    }

    /// Likewise arithmetic negation binds tighter than `*`: `-a * b` is `(-a) * b`.
    #[test]
    fn negation_binds_tighter_than_multiply() {
        match only_expr_kind("-a * b") {
            ExpressionKind::Binary(BinaryOp::Mul, lhs, rhs) => {
                assert!(matches!(lhs.kind, ExpressionKind::Unary(UnaryOp::Neg, _)));
                assert!(matches!(rhs.kind, ExpressionKind::Var(ref v) if v == "b"));
            }
            other => panic!("expected Mul, got {other:?}"),
        }
    }

    /// A binary condition still lands in the condition slot — position, not shape,
    /// discriminates the branches.
    #[test]
    fn if_with_binary_condition_keeps_positional_mapping() {
        match only_expr_kind("if (a < b) 1 else 2") {
            ExpressionKind::If {
                condition,
                false_pathway,
                ..
            } => {
                assert!(matches!(
                    condition.kind,
                    ExpressionKind::Binary(BinaryOp::Lt, _, _)
                ));
                assert!(false_pathway.is_some(), "else branch present");
            }
            other => panic!("expected If, got {other:?}"),
        }
    }
}
