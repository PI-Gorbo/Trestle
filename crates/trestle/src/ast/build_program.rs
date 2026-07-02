//! Walker for `Rule::program` → [`Program`].

use pest::iterators::Pair;

use crate::Rule;

use super::Program;
use super::build_statement::build_let;

/// Build a `Program` from a `Rule::program` pair.
pub fn build_program(pair: Pair<Rule>) -> Program {
    Program {
        statements: pair.into_inner().fold(Vec::new(), |mut statements, pair| {
            match pair.as_rule() {
                Rule::statement => statements.push(build_let(pair)),
                Rule::EOI => {}
                rule => unreachable!("unexpected rule in program: {:?}", rule),
            }

            statements
        }),
    }
}
