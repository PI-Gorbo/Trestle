//! Walker for `Rule::program` → [`Program`].
use miette::{Diagnostic, SourceSpan};
use pest::iterators::Pair;
use thiserror::Error;

use super::Rule;
use super::ast::source_span_from_pest_span;
use super::build_expression::build_expr;

use super::ast::LoweredProgram;

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

    #[error("internal invariant violated")]
    #[diagnostic(code(trestle::invariant))]
    Invariant { span: SourceSpan },
}

/// Build a `Program` from a `Rule::program` pair.
pub fn build_program(pair: Pair<Rule>) -> Result<LoweredProgram, BuildError> {
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

    Ok(LoweredProgram { expressions })
}
