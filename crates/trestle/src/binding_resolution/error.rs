#![allow(unused_assignments)]

use miette::{Diagnostic, SourceSpan};
use thiserror::Error;

#[derive(Error, Diagnostic, Debug)]
pub enum BindingResolutionError {
    #[error("`{name}` is not defined")]
    #[diagnostic(code(trestle::unbound_name))]
    UnboundName {
        name: String,
        #[label("used here")]
        span: SourceSpan,
    },

    #[error("`{name}` is not a defined type")]
    #[diagnostic(code(trestle::unbound_type_name))]
    UnboundTypeName {
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
