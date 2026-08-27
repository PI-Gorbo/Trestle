use miette::{IntoDiagnostic, NamedSource, Report, Result};
use pest::Parser;
use pest_derive::Parser;

pub mod ast;
mod build_expression;
mod build_program;

pub use build_program::{BuildError, build_program};

#[derive(Parser)]
#[grammar = "parse/trestle.pest"]
pub struct TrestleParser;

pub fn parse(src: &str) -> Result<ast::ParsedProgram> {
    let program_pair = TrestleParser::parse(Rule::program, src)
        .into_diagnostic()?
        .next()
        .expect("the program rule always yields exactly one pair");
    build_program(program_pair)
        .map_err(|e| Report::new(e).with_source_code(NamedSource::new("input", src.to_string())))
}
