//! Pass 2 — type checking. Turns a [`ResolvedProgram`] into an [`AnalysedProgram`] by computing
//! a [`Type`](super::analysed::Type) for every node and interpreting annotations.
//!
//! Intended implementation:
//! - A bidirectional walk `check_expr(expr, expected: Option<&Type>) -> analysed::Expression`:
//!   *synthesise* a node's type bottom-up where its form determines it (literals, annotated
//!   params, applications of known functions), and *check* against `expected` where a type flows
//!   in from context (an annotated `let`, or a call site). This is what lets an unannotated
//!   parameter be typed when — and only when — it sits in a checking position.
//! - Interpret [`ast::TypeDeclaration::Named`](crate::parse::ast::TypeDeclaration) into a
//!   concrete [`Type`] (`"Int"`/`"Bool"`/`"String"` → [`Literal`](super::analysed::Literal);
//!   unknown → an error). This is where type-*name* resolution will grow when generics arrive.
//! - Fold a `FunctionInvocation`'s argument `Vec` left-to-right through the callee's curried
//!   `Fn(a, Fn(b, r))` type, peeling one arrow (and checking one argument) per element.
//! - Build the analysed [`BindingInfo`](super::analysed::BindingInfo) table by pairing each
//!   [`ResolvedBinding`](super::resolved::ResolvedBinding)'s name+span with its computed type.
//! - Report mismatches as [`AnalysisError::TypeMismatch`]; collect rather than bail.

use super::AnalysisError;
use super::analysed::AnalysedProgram;
use super::resolved::ResolvedProgram;

/// Type-check a name-resolved program into a fully typed [`AnalysedProgram`].
pub(super) fn type_check(program: &ResolvedProgram) -> Result<AnalysedProgram, Vec<AnalysisError>> {
    let _ = program;
    todo!()
}
