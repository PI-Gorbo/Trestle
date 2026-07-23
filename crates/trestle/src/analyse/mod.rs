//! Static analysis: the linear pipeline that turns a parsed program
//! ([`ast::ParsedProgram`]) into a [`TypeCheckedProgram`] by running the two analysis passes in
//! order.
//!
//! 1. [`binding_resolution`](crate::binding_resolution) — assign a unique
//!    [`BindingId`](crate::binding_resolution::BindingId) per binding and replace every name with
//!    its id (name resolution only; no type logic).
//! 2. [`type_check`](crate::type_check) — bidirectional type synthesis/checking over the resolved
//!    tree.
//!
//! The split is deliberate: name resolution never depends on types and never grows, while type
//! checking grows with every future feature — so keeping them apart means new type-system work
//! only ever touches the second pass. Mirrors the resolver → typechecker split in Rust/GHC. Each
//! pass lives in its own folder with its own error type; this module composes them and tags a
//! failure with the phase it came from.

use crate::binding_resolution::{self, BindingResolutionError};
use crate::parse::ast::ParsedProgram;
use crate::type_check::{self, TypeCheckError, TypeCheckedProgram};

/// A failure from one of the analysis passes. The pipeline fails fast, so a batch is always from a
/// single phase — the variant says which.
#[derive(Debug)]
pub enum AnalysisError {
    BindingResolution(Vec<BindingResolutionError>),
    TypeCheck(Vec<TypeCheckError>),
}

/// Resolve names and type-check a parsed program into a [`TypeCheckedProgram`].
///
/// Binding resolution runs first, mapping every name to a
/// [`BindingId`](crate::binding_resolution::BindingId); type checking then types the resolved tree.
/// A failure in either pass surfaces as a phase-tagged [`AnalysisError`].
pub fn analyse(program: ParsedProgram) -> Result<TypeCheckedProgram, AnalysisError> {
    let resolved = binding_resolution::resolve(program).map_err(AnalysisError::BindingResolution)?;

    type_check::analyse(resolved).map_err(AnalysisError::TypeCheck)
}
