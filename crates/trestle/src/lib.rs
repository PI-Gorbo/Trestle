//! Trestle language front-end: the `pest` grammar, its generated parser, and the AST.
//!
//! The binary (`main.rs`) and the conformance suite (`trestle-tests` crate) both
//! drive the language through this crate's public API — chiefly [`parse`].

use pest::Parser;
use pest_derive::Parser;

pub mod ast;

/// The `pest`-generated parser. `Rule` (the grammar's rule enum) is generated
/// alongside it and re-exported implicitly via the derive.
#[derive(Parser)]
#[grammar = "trestle.pest"]
pub struct TrestleParser;

/// Parse Trestle source text into a [`ast::Program`].
///
/// The `pest` error is boxed because it is large relative to the `Ok` variant.
pub fn parse(src: &str) -> Result<ast::Program, Box<pest::error::Error<Rule>>> {
    let program_pair = TrestleParser::parse(Rule::program, src)?
        .next()
        .expect("the program rule always yields exactly one pair");
    Ok(ast::build_program(program_pair))
}
