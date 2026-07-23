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
