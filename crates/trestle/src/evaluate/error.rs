//! Isolated in its own module so the `#![allow(unused_assignments)]` below stays local. The
//! `thiserror`/`miette` derives emit per-field assignments that trip `unused_assignments` on
//! fields not yet read, and only a *module*-scoped allow suppresses it (item- and field-level
//! allows don't, due to the derive's span hygiene).
#![allow(unused_assignments)]

use miette::{Diagnostic, SourceSpan};
use std::panic::Location;
use thiserror::Error;

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

    #[error("division by zero")]
    #[diagnostic(
        code(trestle::division_by_zero),
        help("the right-hand side of `/` evaluated to 0")
    )]
    DivisionByZero {
        #[label("this division has a zero divisor")]
        span: SourceSpan,
    },

    #[error("arithmetic overflow evaluating `{operator}`")]
    #[diagnostic(
        code(trestle::arithmetic_overflow),
        help("Trestle integers are signed 64-bit: {lhs} {operator} {rhs} does not fit")
    )]
    ArithmeticOverflow {
        operator: &'static str,
        lhs: i64,
        rhs: i64,
        #[label("this operation overflows")]
        span: SourceSpan,
    },
}

impl EvalError {
    #[track_caller]
    pub fn invariant_due_to_type_check(span: SourceSpan, context: &'static str) -> Self {
        Self::InvariantDueToTypeCheck {
            context,
            location: Location::caller(),
            span,
        }
    }
}
