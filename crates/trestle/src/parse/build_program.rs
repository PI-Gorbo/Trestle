//! Walker for `Rule::program` → [`ParsedProgram`].
use pest::iterators::Pair;

use super::Rule;
use super::ast::source_span_from_pest_span;
use super::build_expression::build_expr;

use super::ast::ParsedProgram;

pub use error::BuildError;

/// Isolated in its own module so the `#![allow(unused_assignments)]` below stays local. The
/// `thiserror`/`miette` derives emit per-field assignments that trip `unused_assignments` on
/// fields not yet read, and only a *module*-scoped allow suppresses it (item- and field-level
/// allows don't, due to the derive's span hygiene). Same pattern as `analyse::error`.
mod error {
    #![allow(unused_assignments)]

    use super::Rule;
    use miette::{Diagnostic, SourceSpan};
    use thiserror::Error;

    /// A structural error hit while walking the parse tree into the AST.
    ///
    /// This is a *lightweight* diagnostic: it carries only the offending `span`
    /// (a `#[label]`) and `rule`, but no `#[source_code]`. The source text is
    /// attached later, at the `parse()` boundary, via `Report::with_source_code`.
    #[derive(Error, Diagnostic, Debug)]
    pub enum BuildError {
        #[error("unexpected rule {rule:?}")]
        #[diagnostic(code(trestle::unexpected_rule))]
        UnexpectedRule {
            rule: Rule,
            #[label("unexpected here")]
            span: SourceSpan,
        },

        #[error("lambda is missing a body")]
        #[diagnostic(code(trestle::missing_lambda_body))]
        MissingLambdaBody {
            #[label("this lambda has no body")]
            span: SourceSpan,
        },

        #[error("let is missing a body")]
        #[diagnostic(code(trestle::missing_let_body))]
        MissingLetBody {
            #[label("this let has no body")]
            span: SourceSpan,
        },

        #[error("parameter `{name}` requires a type annotation")]
        #[diagnostic(code(trestle::missing_param_type))]
        MissingParamType {
            name: String,
            #[label("this parameter needs a type, e.g. `{name}: Int`")]
            span: SourceSpan,
        },

        #[error("invalid escape sequence in string literal: {message}")]
        #[diagnostic(code(trestle::invalid_string_escape))]
        InvalidStringEscape {
            message: String,
            #[label("this string has an invalid escape sequence")]
            span: SourceSpan,
        },

        #[error("record type declares field `{name}` more than once")]
        #[diagnostic(code(trestle::duplicate_record_field))]
        DuplicateRecordField {
            name: String,
            #[label("this field is already declared in this record")]
            span: SourceSpan,
        },

        #[error("internal invariant violated")]
        #[diagnostic(code(trestle::invariant))]
        Invariant { span: SourceSpan },
    }
}

/// Build a `ParsedProgram` from a `Rule::program` pair.
pub fn build_program(pair: Pair<Rule>) -> Result<ParsedProgram, BuildError> {
    let expressions = pair
        .into_inner()
        .try_fold(Vec::new(), |mut statements, pair| {
            let span = pair.as_span();

            match pair.as_rule() {
                Rule::expr => {
                    // build_expr returns a fully-formed, source-spanned Expression.
                    statements.push(build_expr(pair)?);

                    Ok(statements)
                }
                Rule::EOI => Ok(statements),
                rule => Err(BuildError::UnexpectedRule {
                    rule,
                    span: source_span_from_pest_span(span),
                }),
            }
        })?;

    Ok(ParsedProgram { expressions })
}

#[cfg(test)]
mod tests {
    use crate::parse::ast::{BinaryOp, Expression, ExpressionKind, Literal, UnaryOp};
    use crate::parse::parse;

    /// One kind per top-level expression — the program-level view `build_program` produces.
    fn program_kinds(source: &str) -> Vec<ExpressionKind> {
        parse(source)
            .expect("source parses")
            .expressions
            .into_iter()
            .map(|expr| expr.kind)
            .collect()
    }

    /// The sole top-level expression, for sources that are a single chain.
    fn only_expr(source: &str) -> Expression {
        let mut expressions = parse(source).expect("source parses").expressions;
        assert_eq!(
            expressions.len(),
            1,
            "expected one top-level expression, got {expressions:?}"
        );

        expressions.pop().expect("one top-level expression")
    }

    fn only_kind(source: &str) -> ExpressionKind {
        only_expr(source).kind
    }

    /// Unwrap a call into (callee kind, arguments), reporting the shape actually built.
    fn expect_call(kind: ExpressionKind) -> (ExpressionKind, Vec<Expression>) {
        match kind {
            ExpressionKind::FunctionInvocation {
                function,
                arguments,
            } => (function.kind, arguments),
            other => panic!("expected a call, got {other:?}"),
        }
    }

    /// Unwrap a field access into (target kind, field name).
    fn expect_field(kind: ExpressionKind) -> (ExpressionKind, String) {
        match kind {
            ExpressionKind::FieldAccess { target, identifier } => (target.kind, identifier),
            other => panic!("expected a field access, got {other:?}"),
        }
    }

    fn is_var(kind: &ExpressionKind, name: &str) -> bool {
        matches!(kind, ExpressionKind::Var(v) if v == name)
    }

    fn is_int(expr: &Expression, value: usize) -> bool {
        matches!(expr.kind, ExpressionKind::Literal(Literal::Int(n)) if n == value)
    }

    /// The base case: an identifier followed by `(args)` is an invocation of that identifier.
    #[test]
    fn a_call_builds_an_invocation() {
        let (callee, arguments) = expect_call(only_kind("f(1)"));
        assert!(
            is_var(&callee, "f"),
            "expected a call to `f`, got {callee:?}"
        );
        assert_eq!(arguments.len(), 1);
        assert!(is_int(&arguments[0], 1));
    }

    /// The synthesized node covers the base *and* its postfix, so a diagnostic on a call
    /// underlines the whole `f(1)` rather than just the `f`.
    #[test]
    fn a_call_spans_callee_through_closing_paren() {
        let expr = only_expr("f(1)");
        assert_eq!(expr.span.offset(), 0);
        assert_eq!(expr.span.len(), "f(1)".len());
    }

    /// The fold carries the widened span forward, so each postfix in a chain covers
    /// everything to its left — not just the previous postfix.
    #[test]
    fn a_chained_postfix_spans_the_whole_chain() {
        let expr = only_expr("f(1)(2)");
        assert_eq!(expr.span.offset(), 0);
        assert_eq!(expr.span.len(), "f(1)(2)".len());

        let expr = only_expr("a.b.c");
        assert_eq!(expr.span.offset(), 0);
        assert_eq!(expr.span.len(), "a.b.c".len());
    }

    /// `f(a, b)` is sugar for `f(a)(b)`, but that desugaring belongs to type inference
    /// (`apply_arguments`), not the parser: the AST keeps one flat argument list.
    #[test]
    fn multiple_arguments_stay_a_flat_list() {
        let (callee, arguments) = expect_call(only_kind("add(3, 4)"));
        assert!(is_var(&callee, "add"));
        assert_eq!(
            arguments.len(),
            2,
            "expected both arguments on one call, got {arguments:?}"
        );
        assert!(is_int(&arguments[0], 3));
        assert!(is_int(&arguments[1], 4));
    }

    /// Zero-argument calls are real — `closure.trsl` invokes `create_closure()`.
    #[test]
    #[ignore = "call_arguments requires at least one argument; `f()` does not parse"]
    fn a_zero_argument_call_builds_an_empty_invocation() {
        let (callee, arguments) = expect_call(only_kind("f()"));
        assert!(is_var(&callee, "f"));
        assert!(
            arguments.is_empty(),
            "expected no arguments, got {arguments:?}"
        );
    }

    /// The other postfix: `.name` reads a field off whatever precedes it.
    #[test]
    fn a_field_access_builds_a_field_access() {
        let (target, field) = expect_field(only_kind("p.x"));
        assert!(
            is_var(&target, "p"),
            "expected a read off `p`, got {target:?}"
        );
        assert_eq!(field, "x");
    }

    /// Repeated call postfixes nest left-to-right: `f(1)(2)` is `(f(1))(2)`, the shape
    /// partial application relies on.
    #[test]
    fn chained_calls_nest_left_to_right() {
        let (inner, outer_arguments) = expect_call(only_kind("f(1)(2)"));
        assert!(is_int(&outer_arguments[0], 2));

        let (callee, inner_arguments) = expect_call(inner);
        assert!(is_var(&callee, "f"));
        assert!(is_int(&inner_arguments[0], 1));
    }

    /// Likewise for field access: `a.b.c` is `(a.b).c` — the `nested-field-access` target.
    #[test]
    fn chained_field_accesses_nest_left_to_right() {
        let (inner, outer_field) = expect_field(only_kind("a.b.c"));
        assert_eq!(outer_field, "c");

        let (target, inner_field) = expect_field(inner);
        assert!(is_var(&target, "a"));
        assert_eq!(inner_field, "b");
    }

    /// The two postfix kinds interleave — `a.b().c` reads a function-valued field, invokes
    /// it, then reads a field off the result. Trestle has no methods; this is the
    /// `field-call-chain` target.
    #[test]
    #[ignore = "needs both fixes: postfix folding and zero-argument calls"]
    fn a_call_between_field_accesses_chains() {
        let (call, outer_field) = expect_field(only_kind("a.b().c"));
        assert_eq!(outer_field, "c");

        let (callee, arguments) = expect_call(call);
        assert!(arguments.is_empty());

        let (target, inner_field) = expect_field(callee);
        assert!(is_var(&target, "a"));
        assert_eq!(inner_field, "b");
    }

    /// The mirror case: a field read off a call's result.
    #[test]
    fn a_field_access_on_a_call_result_chains() {
        let (call, field) = expect_field(only_kind("f(x).y"));
        assert_eq!(field, "y");

        let (callee, arguments) = expect_call(call);
        assert!(is_var(&callee, "f"));
        assert_eq!(arguments.len(), 1);
    }

    /// Postfixes attach to a `primary`, so they bind tighter than any infix operator:
    /// `p.x + p.y` is `(p.x) + (p.y)`, never `(p.x + p).y`.
    #[test]
    fn postfix_binds_tighter_than_an_infix_operator() {
        match only_kind("p.x + p.y") {
            ExpressionKind::Binary(BinaryOp::Add, lhs, rhs) => {
                assert!(matches!(lhs.kind, ExpressionKind::FieldAccess { .. }));
                assert!(matches!(rhs.kind, ExpressionKind::FieldAccess { .. }));
            }
            other => panic!("expected Add over two field reads, got {other:?}"),
        }
    }

    /// Prefix operators sit outside the `primary` too, so `-f(1)` negates the call's
    /// result rather than calling a negated `f`.
    #[test]
    fn postfix_binds_tighter_than_a_prefix_operator() {
        match only_kind("-f(1)") {
            ExpressionKind::Unary(UnaryOp::Neg, operand) => {
                assert!(matches!(
                    operand.kind,
                    ExpressionKind::FunctionInvocation { .. }
                ));
            }
            other => panic!("expected a negated call, got {other:?}"),
        }
    }

    /// A postfix applies to any `primary_base`, including a parenthesized expression —
    /// the callee needn't be a bare identifier.
    #[test]
    fn a_parenthesized_base_takes_a_postfix() {
        let (callee, arguments) = expect_call(only_kind("(g)(1)"));
        assert!(is_var(&callee, "g"));
        assert!(is_int(&arguments[0], 1));
    }

    /// `build_program`'s own fold. Newlines are insignificant (see `WHITESPACE` in
    /// trestle.pest), so the two statements are delimited structurally: the lambda body
    /// `x` ends at `d`, which starts a fresh top-level expression.
    #[test]
    fn each_top_level_call_is_its_own_expression() {
        let kinds = program_kinds("let d = (x: Int) => x\nd(2)");
        assert_eq!(kinds.len(), 2, "expected a `let` and a call, got {kinds:?}");
        assert!(matches!(kinds[0], ExpressionKind::Let { .. }));

        let mut kinds = kinds.into_iter();
        kinds.next();
        let (callee, arguments) = expect_call(kinds.next().expect("a second expression"));
        assert!(is_var(&callee, "d"));
        assert!(is_int(&arguments[0], 2));
    }
}
