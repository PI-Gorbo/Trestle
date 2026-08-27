use pest::iterators::Pair;

use super::Rule;
use super::ast::source_span_from_pest_span;
use super::build_expression::build_expr;

use super::ast::ParsedProgram;

pub use error::BuildError;

mod error {
    #![allow(unused_assignments)]

    use super::Rule;
    use miette::{Diagnostic, SourceSpan};
    use std::panic::Location;
    use thiserror::Error;

    #[derive(Error, Diagnostic, Debug)]
    pub enum BuildError {
        #[error("unexpected rule {rule:?} while building {context}")]
        #[diagnostic(
            code(trestle::unexpected_rule),
            help("internal trestle error, raised at {location}")
        )]
        UnexpectedRule {
            rule: Rule,
            context: &'static str,
            location: &'static Location<'static>,
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

        #[error("integer literal `{literal}` is out of range")]
        #[diagnostic(
            code(trestle::int_literal_out_of_range),
            help("Trestle integers are signed 64-bit: the largest literal is 9223372036854775807")
        )]
        IntLiteralOutOfRange {
            literal: String,
            #[label("this literal does not fit in an Int")]
            span: SourceSpan,
        },

        #[error("internal invariant violated while building {context}")]
        #[diagnostic(
            code(trestle::invariant),
            help("internal trestle error, raised at {location}")
        )]
        Invariant {
            context: &'static str,
            location: &'static Location<'static>,
            #[label("here")]
            span: SourceSpan,
        },
    }

    impl BuildError {
        #[track_caller]
        pub fn unexpected_rule(rule: Rule, span: SourceSpan, context: &'static str) -> Self {
            Self::UnexpectedRule {
                rule,
                context,
                location: Location::caller(),
                span,
            }
        }

        #[track_caller]
        pub fn invariant(span: SourceSpan, context: &'static str) -> Self {
            Self::Invariant {
                context,
                location: Location::caller(),
                span,
            }
        }
    }
}

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
                rule => Err(BuildError::unexpected_rule(
                    rule,
                    source_span_from_pest_span(span),
                    "a top-level statement",
                )),
            }
        })?;

    Ok(ParsedProgram { expressions })
}

#[cfg(test)]
mod tests {
    use super::{BuildError, Rule};
    use crate::parse::ast::{BinaryOp, Expression, ExpressionKind, Literal, UnaryOp};
    use crate::parse::parse;

    #[test]
    fn unexpected_rule_records_its_construction_site() {
        let expected_line = line!() + 1;
        let error = BuildError::unexpected_rule(Rule::EOI, (0, 1).into(), "a test");

        match error {
            BuildError::UnexpectedRule {
                location, context, ..
            } => {
                assert_eq!(location.file(), file!());
                assert_eq!(location.line(), expected_line);
                assert_eq!(context, "a test");
            }
            other => panic!("expected an UnexpectedRule, got {other:?}"),
        }
    }

    #[test]
    fn invariant_records_its_construction_site() {
        let expected_line = line!() + 1;
        let error = BuildError::invariant((0, 1).into(), "a test");

        match error {
            BuildError::Invariant {
                location, context, ..
            } => {
                assert_eq!(location.file(), file!());
                assert_eq!(location.line(), expected_line);
                assert_eq!(context, "a test");
            }
            other => panic!("expected an Invariant, got {other:?}"),
        }
    }

    #[test]
    fn the_rendered_diagnostic_names_the_rust_call_site() {
        let report = miette::Report::new(BuildError::unexpected_rule(
            Rule::EOI,
            (0, 1).into(),
            "a test",
        ));
        let rendered = format!("{report:?}");

        assert!(
            rendered.contains("build_program.rs"),
            "expected the Rust call site in the rendered diagnostic, got:\n{rendered}"
        );
        assert!(
            rendered.contains("while building a test"),
            "expected the walker context in the rendered diagnostic, got:\n{rendered}"
        );
    }

    fn program_kinds(source: &str) -> Vec<ExpressionKind> {
        parse(source)
            .expect("source parses")
            .expressions
            .into_iter()
            .map(|expr| expr.kind)
            .collect()
    }

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

    fn expect_call(kind: ExpressionKind) -> (ExpressionKind, Vec<Expression>) {
        match kind {
            ExpressionKind::FunctionInvocation {
                function,
                arguments,
            } => (function.kind, arguments),
            other => panic!("expected a call, got {other:?}"),
        }
    }

    fn expect_field(kind: ExpressionKind) -> (ExpressionKind, String) {
        match kind {
            ExpressionKind::FieldAccess { target, identifier } => (target.kind, identifier),
            other => panic!("expected a field access, got {other:?}"),
        }
    }

    fn is_var(kind: &ExpressionKind, name: &str) -> bool {
        matches!(kind, ExpressionKind::Var(v) if v == name)
    }

    fn is_int(expr: &Expression, value: i64) -> bool {
        matches!(expr.kind, ExpressionKind::Literal(Literal::Int(n)) if n == value)
    }

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

    #[test]
    fn a_call_spans_callee_through_closing_paren() {
        let expr = only_expr("f(1)");
        assert_eq!(expr.span.offset(), 0);
        assert_eq!(expr.span.len(), "f(1)".len());
    }

    #[test]
    fn a_chained_postfix_spans_the_whole_chain() {
        let expr = only_expr("f(1)(2)");
        assert_eq!(expr.span.offset(), 0);
        assert_eq!(expr.span.len(), "f(1)(2)".len());

        let expr = only_expr("a.b.c");
        assert_eq!(expr.span.offset(), 0);
        assert_eq!(expr.span.len(), "a.b.c".len());
    }

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

    #[test]
    fn a_zero_argument_call_builds_an_empty_invocation() {
        let (callee, arguments) = expect_call(only_kind("f()"));
        assert!(is_var(&callee, "f"));
        assert!(
            arguments.is_empty(),
            "expected no arguments, got {arguments:?}"
        );
    }

    #[test]
    fn a_paren_after_whitespace_is_not_a_call() {
        let program = parse("1\n\n() => 2").expect("source parses");
        assert_eq!(
            program.expressions.len(),
            2,
            "expected a literal and a lambda, got {:?}",
            program.expressions
        );
        assert!(matches!(
            program.expressions[0].kind,
            ExpressionKind::Literal(Literal::Int(1))
        ));
        assert!(matches!(
            program.expressions[1].kind,
            ExpressionKind::Lambda(_)
        ));
    }

    #[test]
    fn a_field_access_builds_a_field_access() {
        let (target, field) = expect_field(only_kind("p.x"));
        assert!(
            is_var(&target, "p"),
            "expected a read off `p`, got {target:?}"
        );
        assert_eq!(field, "x");
    }

    #[test]
    fn chained_calls_nest_left_to_right() {
        let (inner, outer_arguments) = expect_call(only_kind("f(1)(2)"));
        assert!(is_int(&outer_arguments[0], 2));

        let (callee, inner_arguments) = expect_call(inner);
        assert!(is_var(&callee, "f"));
        assert!(is_int(&inner_arguments[0], 1));
    }

    #[test]
    fn chained_field_accesses_nest_left_to_right() {
        let (inner, outer_field) = expect_field(only_kind("a.b.c"));
        assert_eq!(outer_field, "c");

        let (target, inner_field) = expect_field(inner);
        assert!(is_var(&target, "a"));
        assert_eq!(inner_field, "b");
    }

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

    #[test]
    fn a_field_access_on_a_call_result_chains() {
        let (call, field) = expect_field(only_kind("f(x).y"));
        assert_eq!(field, "y");

        let (callee, arguments) = expect_call(call);
        assert!(is_var(&callee, "f"));
        assert_eq!(arguments.len(), 1);
    }

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

    #[test]
    fn a_parenthesized_base_takes_a_postfix() {
        let (callee, arguments) = expect_call(only_kind("(g)(1)"));
        assert!(is_var(&callee, "g"));
        assert!(is_int(&arguments[0], 1));
    }

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
