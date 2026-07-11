//! Static analysis: resolve names and type-check the LoweredAst
//! ([`ast::LoweredProgram`]) into an [`AnalysedProgram`], in two passes.
//!
//! 1. [`resolve_names`] — assign a unique [`BindingId`](analysed::BindingId) per binding and
//!    replace every name with its id (name resolution only; no type logic).
//! 2. [`type_check`] — bidirectional type synthesis/checking over the resolved tree.
//!
//! The split is deliberate: name resolution never depends on types and never grows, while type
//! checking grows with every future feature — so keeping them apart means new type-system work
//! only ever touches pass 2. Mirrors the resolver → typechecker split in Rust/GHC.

use miette::{Diagnostic, SourceSpan};
use thiserror::Error;

pub mod analysed;
mod resolve_names;
pub mod resolved;
mod type_check;

use crate::parse::ast;
use analysed::{AnalysedProgram, Type};

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

    #[error("`{name}` is already declared in this scope")]
    #[diagnostic(code(trestle::duplicate_binding))]
    DuplicateBinding {
        name: String,
        #[label("redeclared here")]
        span: SourceSpan,
        #[label("first declared here")]
        original_span: SourceSpan,
    },

    #[error("type mismatch: expected {expected:?}, found {found:?}")]
    #[diagnostic(code(trestle::type_mismatch))]
    TypeMismatch {
        expected: Type,
        found: Type,
        #[label("here")]
        span: SourceSpan,
    },

    #[error("`{found:?}` is not a function and cannot be applied to arguments")]
    #[diagnostic(code(trestle::not_a_function))]
    NotAFunction {
        found: Type,
        #[label("called here")]
        span: SourceSpan,
    },

    #[error("this function was applied to too many arguments")]
    #[diagnostic(code(trestle::too_many_arguments))]
    TooManyArguments {
        #[label("too many arguments in this call")]
        span: SourceSpan,
    },

    #[error("this function takes no arguments, but arguments were provided")]
    #[diagnostic(code(trestle::arguments_to_argumentless_function))]
    ArgumentsToArgumentlessFunction {
        #[label("no arguments expected here")]
        span: SourceSpan,
    },

    #[error("unknown type `{name}`")]
    #[diagnostic(code(trestle::unknown_type))]
    UnknownType {
        name: String,
        #[label("unknown type")]
        span: SourceSpan,
    },

    #[error("cannot determine the type of `{name}` add a type annotation")]
    #[diagnostic(code(trestle::missing_annotation))]
    MissingAnnotation {
        name: String,
        #[label("type unknown here")]
        span: SourceSpan,
    },

    #[error("could not resolve the type of the variable `{name}` - this is a system error")]
    #[diagnostic(code(trestle::missing_annotation))]
    UntypedBindingAfterTypeCheck {
        name: String,

        #[label("type unknown here")]
        span: SourceSpan,
    },
}

/// Resolve names and type-check a lowered program into an [`AnalysedProgram`].
///
/// Pass 1 resolves every name to a [`BindingId`](analysed::BindingId); pass 2 types the
/// resolved tree. Errors from either pass surface as a batch.
pub fn analyse(program: ast::LoweredProgram) -> Result<AnalysedProgram, Vec<AnalysisError>> {
    let resolved = resolve_names::resolve(program)?;

    type_check::type_check(resolved)
}
