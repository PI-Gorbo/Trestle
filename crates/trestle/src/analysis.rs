//! Static analysis: resolve names and type-check the LoweredAst
//! ([`ast::Program`](crate::ast::Program)) into a [`CheckedProgram`].

use miette::{Diagnostic, SourceSpan};
use thiserror::Error;

use crate::ast;
use crate::checked::{CheckedProgram, Type};

/// Name-resolution and type-checking failures. Reported as a batch (`Vec`) so the user
/// sees all problems at once. Representative variants — grow as you implement.
#[derive(Error, Diagnostic, Debug)]
pub enum AnalysisError {
    #[error("`{name}` is not defined")]
    #[diagnostic(code(trestle::unbound_name))]
    UnboundName {
        name: String,
        #[label("used here")]
        span: SourceSpan,
    },

    #[error("type mismatch: expected {expected:?}, found {found:?}")]
    #[diagnostic(code(trestle::type_mismatch))]
    TypeMismatch {
        expected: Type,
        found: Type,
        #[label("here")]
        span: SourceSpan,
    },
}

/// Resolve names and type-check a lowered program into a [`CheckedProgram`].
pub fn analyse(program: &ast::Program) -> Result<CheckedProgram, Vec<AnalysisError>> {
    let _ = program;
    todo!()
}
