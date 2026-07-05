//! Trestle language front-end: the `pest` grammar, its generated parser, and the AST.
//!
//! The binary (`main.rs`) and the conformance suite (`trestle-tests` crate) both
//! drive the language through this crate's public API — chiefly [`parse`].

use miette::{IntoDiagnostic, NamedSource, Report, Result};
use pest::Parser;
use pest_derive::Parser;

pub mod analysis;
/// The lowered AST — currying is already desugared here (see [`ast::Lambda`]).
pub mod ast;
pub mod checked;
pub mod evaluation;

/// The `pest`-generated parser. `Rule` (the grammar's rule enum) is generated
/// alongside it and re-exported implicitly via the derive.
#[derive(Parser)]
#[grammar = "trestle.pest"]
pub struct TrestleParser;

/// Parse Trestle source text into a [`ast::Program`].
///
/// Both failure modes are surfaced as a [`miette::Report`]: the `pest` syntax
/// error via [`IntoDiagnostic`], and the AST-walk [`ast::BuildError`] with the
/// source text attached here (the walker itself stays source-free).
pub fn parse(src: &str) -> Result<ast::Program> {
    let program_pair = TrestleParser::parse(Rule::program, src)
        .into_diagnostic()?
        .next()
        .expect("the program rule always yields exactly one pair");
    ast::build_program(program_pair)
        .map_err(|e| Report::new(e).with_source_code(NamedSource::new("input", src.to_string())))
}
