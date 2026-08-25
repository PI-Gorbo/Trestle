//! Isolated in its own module so the `#![allow(unused_assignments)]` below stays local. The
//! `thiserror`/`miette` derives emit per-field assignments that trip `unused_assignments` on
//! fields not yet read, and only a *module*-scoped allow suppresses it (item- and field-level
//! allows don't, due to the derive's span hygiene).
#![allow(unused_assignments)]

use super::typed_ast::{Type, TypeVarId};
use miette::{Diagnostic, SourceSpan};
use thiserror::Error;

/// Type-checking failures. Reported as a batch (`Vec`) so the user sees all problems at once.
/// Representative variants — grow as you implement.
#[derive(Error, Diagnostic, Debug)]
pub enum TypeCheckError {
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

    #[error("this expression is not callable")]
    #[diagnostic(code(trestle::expression_not_callable))]
    ExpressionNotCallable {
        #[label("expected a function here")]
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

    #[error("the right-hand side of `|>` takes no arguments, so there is nothing to pipe into")]
    #[diagnostic(code(trestle::pipe_into_argumentless_function))]
    PipeIntoArgumentlessFunction {
        #[label("this takes no arguments")]
        span: SourceSpan,
    },

    #[error(
        "function shape mismatch: one function takes a parameter and the other does not (expected {expected:?}, found {found:?})"
    )]
    #[diagnostic(code(trestle::function_parameter_mismatch))]
    FunctionParameterMismatch {
        expected: Type,
        found: Type,
        #[label("here")]
        span: SourceSpan,
    },

    #[error("record field mismatch: missing {missing:?}, unexpected {additional:?}")]
    #[diagnostic(code(trestle::record_field_mismatch))]
    RecordFieldMismatch {
        missing: Vec<String>,
        additional: Vec<String>,
        #[label("here")]
        span: SourceSpan,
    },

    #[error("`{found:?}` is not a record, so it has no fields to access")]
    #[diagnostic(code(trestle::not_a_record))]
    NotARecord {
        found: Type,
        #[label("field accessed here")]
        span: SourceSpan,
    },

    #[error("no field `{field_name}` on this record (available: {available:?})")]
    #[diagnostic(code(trestle::record_does_not_have_field))]
    RecordDoesNotHaveField {
        field_name: String,
        available: Vec<String>,
        #[label("unknown field")]
        span: SourceSpan,
    },

    #[error("cannot construct an infinite type: `_{}` occurs in `{:?}`", .var.0, .ty)]
    #[diagnostic(code(trestle::infinite_type))]
    InfiniteType {
        var: TypeVarId,
        ty: Type,
        #[label("this type would contain itself")]
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

    #[error("internal compiler error: {message}")]
    #[diagnostic(code(trestle::internal_error))]
    InternalError {
        message: String,
        #[label("while type-checking this")]
        span: SourceSpan,
    },
}
