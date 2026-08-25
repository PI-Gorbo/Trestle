//! Isolated in its own module so the `#![allow(unused_assignments)]` below stays local. The
//! `thiserror`/`miette` derives emit per-field assignments that trip `unused_assignments` on
//! fields not yet read, and only a *module*-scoped allow suppresses it (item- and field-level
//! allows don't, due to the derive's span hygiene).
#![allow(unused_assignments)]

use miette::{Diagnostic, SourceSpan};
use std::panic::Location;
use thiserror::Error;

/// Runtime failures. Evaluation fails fast at the first fault, so this is reported singly rather
/// than batched like the analysis errors.
///
/// Every variant so far records a guarantee `analyse` was supposed to have established — a
/// well-typed program should never reach one. They are errors rather than panics so that a bug in
/// the type checker surfaces as a diagnostic pointing at the offending expression instead of
/// aborting the process. Later tiers (overflow, effects) will add variants that a *correct*
/// program can hit.
#[derive(Error, Diagnostic, Debug)]
pub enum EvalError {
    #[error("internal trestle error: {context}")]
    #[diagnostic(
        code(trestle::invariant_due_to_type_check),
        help("the type checker should have ruled this out; raised at {location}")
    )]
    InvariantDueToTypeCheck {
        context: &'static str,
        location: &'static Location<'static>,
        #[label("while evaluating this")]
        span: SourceSpan,
    },
}

impl EvalError {
    /// Build an `InvariantDueToTypeCheck`, capturing the *Rust* call site.
    ///
    /// The `span` alone only says where in the *Trestle* program the walker faulted; what you
    /// actually need to fix a checker/evaluator mismatch is the arm that trusted the wrong
    /// guarantee. `#[track_caller]` gets that for free — `Location::caller()` resolves to the
    /// constructor call rather than to this function. Same pattern as `BuildError::invariant`.
    #[track_caller]
    pub fn invariant_due_to_type_check(span: SourceSpan, context: &'static str) -> Self {
        Self::InvariantDueToTypeCheck {
            context,
            location: Location::caller(),
            span,
        }
    }
}
