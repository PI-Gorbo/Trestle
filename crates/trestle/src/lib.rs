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

/// Resolve names and type-check a parsed program into a [`TypeCheckedProgram`].
///
/// Phase 2 — the linear pipeline that turns a parsed program ([`ParsedProgram`]) into a
/// [`TypeCheckedProgram`] by running the two analysis passes in order:
///
/// 1. [`binding_resolution`] — assign a unique
///    [`BindingId`](crate::binding_resolution::BindingId) per binding and replace every name with
///    its id (name resolution only; no type logic).
/// 2. [`type_check`] — bidirectional type synthesis/checking over the resolved tree.
///
/// The split is deliberate: name resolution never depends on types and never grows, while type
/// checking grows with every future feature — so keeping them apart means new type-system work
/// only ever touches the second pass. Mirrors the resolver → typechecker split in Rust/GHC. Each
/// pass lives in its own folder with its own error type; a failure in either pass surfaces as a
/// phase-tagged [`AnalysisError`].
pub fn analyse(program: ParsedProgram) -> Result<TypeCheckedProgram, AnalysisError> {
    let resolved = binding_resolution::resolve(program).map_err(AnalysisError::BindingResolution)?;

    type_check::type_check(resolved).map_err(AnalysisError::TypeCheck)
}
