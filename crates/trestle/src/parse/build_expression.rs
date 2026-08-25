use super::{BuildError, Rule};
use crate::parse::ast::{BinaryOp, Literal, TypeExpressionKind, UnaryOp};
use pest::pratt_parser::{Assoc, Op, PrattParser};
use pest::{Span, iterators::Pair};
use std::collections::BTreeMap;
use std::sync::LazyLock;

use super::ast::{
    Expression, ExpressionKind, Lambda, Param, TypeExpression, get_bindings, merge_spans,
    source_span_from_pest_span,
};

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

fn spanned(span: Span, kind: ExpressionKind) -> Expression {
    Expression {
        kind,
        span: source_span_from_pest_span(span),
    }
}

pub fn build_expr(pair: Pair<Rule>) -> Result<Expression, BuildError> {
    let expr_binding = get_bindings(pair, "expression to have bindings");
    match expr_binding.as_rule() {
        Rule::type_declaration_expression => build_type_declaration(expr_binding),
        Rule::list_of_expressions => build_list_of_expressions(expr_binding),
        Rule::let_expression => build_let(expr_binding),
        Rule::lambda_expression => build_lambda(expr_binding),
        Rule::binary_expression => build_binary(expr_binding),
        Rule::if_expression => build_if_expression(expr_binding),
        rule => Err(BuildError::unexpected_rule(
            rule,
            source_span_from_pest_span(expr_binding.as_span()),
            "an expression",
        )),
    }
}

fn build_type_declaration(expr_binding: Pair<Rule>) -> Result<Expression, BuildError> {
    let span = expr_binding.as_span();
    let mut inner = expr_binding.into_inner();

    let identifier = inner
        .next()
        .expect("type_declaration_expression has an identifier")
        .as_str()
        .to_string();
    let type_expression = build_type_expression(
        inner
            .next()
            .expect("type_declaration_expression has a type expression"),
    )?;

    Ok(spanned(
        span,
        ExpressionKind::TypeDeclaration {
            identifier,
            type_expression,
        },
    ))
}

fn build_type_expression(pair: Pair<Rule>) -> Result<TypeExpression, BuildError> {
    let inner = get_bindings(pair, "type expression to have an inner type");
    match inner.as_rule() {
        Rule::record_type_expression => build_record_type_expression(inner),
        Rule::type_identifier => Ok(build_type_identifier_expression(inner)),
        rule => Err(BuildError::unexpected_rule(
            rule,
            source_span_from_pest_span(inner.as_span()),
            "a type expression",
        )),
    }
}

fn build_record_type_expression(pair: Pair<Rule>) -> Result<TypeExpression, BuildError> {
    let mut fields = BTreeMap::new();
    let span = pair.as_span().clone();
    for field in pair.into_inner() {
        let span = source_span_from_pest_span(field.as_span());
        let (key, value) = build_required_binding_target(field)?;

        // Keyed by name
        if fields.insert(key.clone(), value).is_some() {
            return Err(BuildError::DuplicateRecordField { name: key, span });
        }
    }

    Ok(TypeExpression {
        kind: TypeExpressionKind::Record(fields),
        span: source_span_from_pest_span(span),
    })
}

fn build_list_of_expressions(pair: Pair<Rule>) -> Result<Expression, BuildError> {
    let span = pair.as_span();
    let expressions = pair.into_inner().try_fold(Vec::new(), |mut list, expr| {
        list.push(build_expr(expr)?);
        Ok(list)
    })?;
    Ok(spanned(span, ExpressionKind::Block(expressions)))
}

fn build_let(pair: Pair<Rule>) -> Result<Expression, BuildError> {
    let span = pair.as_span();

    let (name, type_dec, value) = pair.into_inner().try_fold(
        (String::new(), None, None),
        |(mut name, mut type_dec, mut value), p| {
            match p.as_rule() {
                Rule::let_kw => {}
                Rule::identifier_with_optional_type_declaration => {
                    (name, type_dec) = build_binding_target(p)?;
                }
                Rule::expr => value = Some(build_expr(p)?),
                rule => {
                    return Err(BuildError::unexpected_rule(
                        rule,
                        source_span_from_pest_span(p.as_span()),
                        "a let binding",
                    ));
                }
            }
            Ok((name, type_dec, value))
        },
    )?;

    match value {
        Some(expr) => Ok(spanned(
            span,
            ExpressionKind::Let {
                name,
                type_dec,
                value: Box::new(expr),
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
                return Err(BuildError::unexpected_rule(
                    rule,
                    source_span_from_pest_span(p.as_span()),
                    "a lambda",
                ));
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

fn build_binding_target(pair: Pair<Rule>) -> Result<(String, Option<TypeExpression>), BuildError> {
    pair.into_inner()
        .try_fold((String::new(), None), |(mut name, mut type_dec), p| {
            match p.as_rule() {
                Rule::identifier => name = p.as_str().to_string(),
                Rule::type_declaration => type_dec = Some(build_type_identifier_expression(p)),
                rule => {
                    return Err(BuildError::unexpected_rule(
                        rule,
                        source_span_from_pest_span(p.as_span()),
                        "a binding target",
                    ));
                }
            }
            Ok((name, type_dec))
        })
}

fn build_required_binding_target(pair: Pair<Rule>) -> Result<(String, TypeExpression), BuildError> {
    let span = source_span_from_pest_span(pair.as_span());

    let (name, type_dec) = build_binding_target(pair)?;
    let Some(type_expression) = type_dec else {
        return Err(BuildError::invariant(span, "a record field"));
    };

    Ok((name, type_expression))
}

fn build_param(pair: Pair<Rule>) -> Result<Param, BuildError> {
    let (name, type_dec) = build_binding_target(pair)?;
    Ok(Param { name, type_dec })
}

fn build_type_opt(pair: Pair<Rule>) -> Option<TypeExpression> {
    pair.into_inner()
        .next()
        .map(build_type_identifier_expression)
}

fn build_type_identifier_expression(pair: Pair<Rule>) -> TypeExpression {
    let span = pair.as_span().clone();
    let ident = pair
        .into_inner()
        .next()
        .expect("type_declaration has an identifier");

    TypeExpression {
        kind: TypeExpressionKind::Named(ident.as_str().to_string()),
        span: source_span_from_pest_span(span),
    }
}

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
                    return Err(BuildError::unexpected_rule(
                        rule,
                        source_span_from_pest_span(op.as_span()),
                        "an infix operator",
                    ));
                }
            };
            Ok(Expression {
                kind: ExpressionKind::Binary(binary_op, Box::new(lhs), Box::new(rhs)),
                span,
            })
        })
        .map_prefix(|op, rhs| {
            let rhs = rhs?;
            let span = merge_spans(source_span_from_pest_span(op.as_span()), rhs.span);
            let unary_op = match op.as_rule() {
                Rule::negate => UnaryOp::Neg,
                Rule::logical_not => UnaryOp::Not,
                rule => {
                    return Err(BuildError::unexpected_rule(
                        rule,
                        source_span_from_pest_span(op.as_span()),
                        "a prefix operator",
                    ));
                }
            };
            Ok(Expression {
                kind: ExpressionKind::Unary(unary_op, Box::new(rhs)),
                span,
            })
        })
        .parse(pair.into_inner())
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
        Rule::record => build_record(child),
        rule => Err(BuildError::unexpected_rule(
            rule,
            source_span_from_pest_span(span),
            "a literal",
        )),
    }
}

fn build_record(pair: Pair<Rule>) -> Result<Expression, BuildError> {
    let span = pair.as_span();
    let mut inner = pair.into_inner();

    // `record` flattens to identifier, expr, identifier, expr, … — walk it two at a time.
    let mut fields = std::iter::from_fn(|| Some((inner.next()?, inner.next()?)));

    let record_values = fields.try_fold(BTreeMap::new(), |mut state, (identifier, expr)| {
        let field_span = source_span_from_pest_span(identifier.as_span());
        let identifier = identifier.as_str().to_string();
        let expr = build_expr(expr)?;

        // Keyed by name, so a repeat would silently overwrite the first.
        if state.insert(identifier.clone(), expr).is_some() {
            return Err(BuildError::DuplicateRecordField {
                name: identifier,
                span: field_span,
            });
        }

        Ok(state)
    })?;

    Ok(spanned(
        span,
        ExpressionKind::Literal(Literal::Record(record_values)),
    ))
}

fn build_primary(pair: Pair<Rule>) -> Result<Expression, BuildError> {
    let mut pairs = pair.into_inner();

    // Pull the 'primary_base' off the stack of pairs, then step through it to the
    // literal / identifier / parenthesised expr it wraps.
    let primary_base_pair = get_bindings(
        pairs.next().expect("primary has one child"),
        "primary to have a base",
    );
    let primary_base_span = primary_base_pair.as_span();
    let primary_base_expr = match primary_base_pair.as_rule() {
        Rule::literal => build_literal(primary_base_pair),
        Rule::identifier => Ok(spanned(
            primary_base_span,
            ExpressionKind::Var(primary_base_pair.as_str().to_string()),
        )),
        Rule::expr => build_expr(primary_base_pair), // parenthesized expression
        rule => Err(BuildError::unexpected_rule(
            rule,
            source_span_from_pest_span(primary_base_span),
            "a primary base",
        )),
    }?;

    // Fold the base expr with the zero or more postfix_pairs that come through.
    // Each fold iteration wraps the current state.
    pairs.try_fold(primary_base_expr, |expr, _primary_postfix| {
        let primary_postfix_rule = _primary_postfix.as_rule();
        let primary_postfix_span = _primary_postfix.as_span();
        let Rule::primary_postfix = primary_postfix_rule else {
            return Err(BuildError::unexpected_rule(
                primary_postfix_rule,
                source_span_from_pest_span(_primary_postfix.as_span()),
                "a primary postfix",
            ));
        };

        let inner_pair = _primary_postfix.into_inner().next().ok_or_else(|| {
            BuildError::invariant(
                source_span_from_pest_span(primary_postfix_span),
                "a primary postfix",
            )
        })?;
        let inner_pair_rule = inner_pair.as_rule();
        match inner_pair_rule {
            Rule::field_access => build_field_access(expr, inner_pair),
            Rule::call_arguments => build_call_arguments(expr, inner_pair),
            rule => Err(BuildError::unexpected_rule(
                rule,
                source_span_from_pest_span(inner_pair.as_span()),
                "a primary postfix",
            )),
        }
    })
}

fn build_field_access(
    target: Expression,
    postfix_pair: Pair<Rule>,
) -> Result<Expression, BuildError> {
    let postfix_pair_span = postfix_pair.as_span();
    let identifier = postfix_pair
        .into_inner()
        .next()
        .map(|v| v.as_str().to_string())
        .ok_or_else(|| {
            BuildError::invariant(
                source_span_from_pest_span(postfix_pair_span),
                "a field access",
            )
        })?;

    // The postfix pair covers only `.name`; the synthesized node covers the target too.
    let span = merge_spans(target.span, source_span_from_pest_span(postfix_pair_span));

    Ok(Expression {
        kind: ExpressionKind::FieldAccess {
            target: Box::new(target),
            identifier,
        },
        span,
    })
}

fn build_call_arguments(
    target: Expression,
    postfix_pair: Pair<Rule>,
) -> Result<Expression, BuildError> {
    //  call_arguments has at most one element, comma_separated_list_of_expressions —
    //  a nullary call `f()` has none.
    let postfix_pair_span = postfix_pair.as_span();
    let arguments = match postfix_pair.into_inner().next() {
        Some(list) => list
            .into_inner()
            .try_fold(Vec::new(), |mut arguments, pair| {
                let expr = build_expr(pair)?;
                arguments.push(expr);
                Ok(arguments)
            })?,
        None => Vec::new(),
    };

    // The postfix pair covers only `(args)`; the synthesized node covers the callee too.
    let span = merge_spans(target.span, source_span_from_pest_span(postfix_pair_span));

    Ok(Expression {
        kind: ExpressionKind::FunctionInvocation {
            function: Box::new(target),
            arguments,
        },
        span,
    })
}

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

    /// Record fields are keyed by name, so a repeated field would silently
    /// overwrite the first. It's rejected at build time instead.
    #[test]
    fn duplicate_record_field_reports_diagnostic() {
        let report =
            parse("type T = { x: Int, x: String }").expect_err("duplicate field must be rejected");
        let rendered = format!("{report:?}");
        assert!(
            rendered.contains("more than once"),
            "expected a duplicate-field diagnostic, got:\n{rendered}"
        );
    }

    /// A record literal keeps every field, keyed by name, with each value built as
    /// its own expression.
    #[test]
    fn record_literal_collects_all_fields() {
        match only_expr_kind(r#"{ x: 1, y: "a" }"#) {
            ExpressionKind::Literal(Literal::Record(fields)) => {
                assert_eq!(fields.len(), 2, "expected two fields, got {fields:?}");
                assert!(matches!(
                    fields["x"].kind,
                    ExpressionKind::Literal(Literal::Int(1))
                ));
                assert!(
                    matches!(fields["y"].kind, ExpressionKind::Literal(Literal::String(ref s)) if s == "a")
                );
            }
            other => panic!("expected record literal, got {other:?}"),
        }
    }

    /// Like record *types*, a record literal is keyed by name — a repeated field
    /// would silently overwrite the first, so it's rejected at build time.
    #[test]
    fn duplicate_record_literal_field_reports_diagnostic() {
        let report = parse("{ x: 1, x: 2 }").expect_err("duplicate field must be rejected");
        let rendered = format!("{report:?}");
        assert!(
            rendered.contains("more than once"),
            "expected a duplicate-field diagnostic, got:\n{rendered}"
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
