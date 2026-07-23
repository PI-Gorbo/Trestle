//! Isolated in its own module so the `#![allow(unused_assignments)]` below stays local. The
//! `thiserror`/`miette` derives emit per-field assignments that trip `unused_assignments` on
//! fields not yet read, and only a *module*-scoped allow suppresses it (item- and field-level
//! allows don't, due to the derive's span hygiene).
#![allow(unused_assignments)]

use miette::{Diagnostic, SourceSpan};
use thiserror::Error;

/// Name-resolution failures. Reported as a batch (`Vec`) so the user sees all problems at once.
#[derive(Error, Diagnostic, Debug)]
pub enum BindingResolutionError {
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
}
