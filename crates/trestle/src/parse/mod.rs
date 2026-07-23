//! Trestle language front-end: the `pest` grammar, its generated parser, and the AST.
//!
//! The binary (`main.rs`) and the conformance suite (`trestle-tests` crate) both
//! drive the language through this crate's public API — chiefly [`parse`].

use miette::{IntoDiagnostic, NamedSource, Report, Result};
use pest::Parser;
use pest_derive::Parser;

/// The parsed AST — currying is already desugared here (see [`ast::Lambda`]).
pub mod ast;
mod build_expression;
mod build_program;

pub use build_program::{BuildError, build_program};

/// The `pest`-generated parser. `Rule` (the grammar's rule enum) is generated
/// alongside it and re-exported implicitly via the derive.
#[derive(Parser)]
#[grammar = "parse/trestle.pest"]
pub struct TrestleParser;

/// Parse Trestle source text into a [`ast::ParsedProgram`].
///
/// Both failure modes are surfaced as a [`miette::Report`]: the `pest` syntax
/// error via [`IntoDiagnostic`], and the AST-walk [`BuildError`] with the
/// source text attached here (the walker itself stays source-free).
pub fn parse(src: &str) -> Result<ast::ParsedProgram> {
    let program_pair = TrestleParser::parse(Rule::program, src)
        .into_diagnostic()?
        .next()
        .expect("the program rule always yields exactly one pair");
    build_program(program_pair)
        .map_err(|e| Report::new(e).with_source_code(NamedSource::new("input", src.to_string())))
}
