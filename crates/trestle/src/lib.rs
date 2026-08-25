//! Trestle language: the three compiler phases.
//!
//! Source flows [`parse()`] → [`analyse()`] → [`evaluate::evaluate`]. The binary
//! (`main.rs`) and the conformance suite (`trestle-tests` crate) drive the language
//! through these public entry points; [`parse()`] is re-exported here for convenience.

// Phase 1 — grammar, parser, and the parsed AST.
pub mod parse;
// Phase 2a — name resolution (parsed AST → binding-resolved AST).
pub mod binding_resolution;
// Phase 2b — type checking (binding-resolved AST → type-checked AST).
pub mod type_check;
// Phase 3 — tree-walk evaluation.
pub mod evaluate;
// The bindings every program starts with. Above the phases because both analysis passes read it.
pub mod prelude;

pub use parse::parse;

use crate::binding_resolution::BindingResolutionError;
use crate::parse::ast::ParsedProgram;
use crate::type_check::{TypeCheckError, TypeCheckedProgram};

/// A failure from one of the analysis passes. The pipeline fails fast, so a batch is always from a
/// single phase — the variant says which.
#[derive(Debug)]
pub enum AnalysisError {
    BindingResolution(Vec<BindingResolutionError>),
    TypeCheck(Vec<TypeCheckError>),
}

// Perform Binding Resolution for types and variables, and then type check.
pub fn analyse(program: ParsedProgram) -> Result<TypeCheckedProgram, AnalysisError> {
    let resolved =
        binding_resolution::resolve(program).map_err(AnalysisError::BindingResolution)?;

    type_check::type_check(resolved).map_err(AnalysisError::TypeCheck)
}
