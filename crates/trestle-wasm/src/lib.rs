//! `trestle-wasm` — the browser-facing surface of the Trestle compiler.
//!
//! Three exports, consumed by `apps/demo`:
//!
//! - [`check`] — parse + analyse. Drives the squiggles as you type.
//! - [`run`] — parse + analyse + evaluate. Drives the Run button.
//! - [`version`] — shown in the playground header so it is obvious which build is loaded.
//!
//! ## On panics
//!
//! We do not try to recover from one here: `panic = "abort"` on wasm means there is nothing to
//! catch. The panic surfaces to JS as a `RuntimeError`, and the caller
//! (`apps/demo/app/lib/compiler/client.ts`) terminates that worker and spawns a fresh one.
//! `console_error_panic_hook` makes the message readable in devtools on the way out.
//!
//! The reachable panics are no longer `todo!()` holes — those are closed — but they have not
//! gone away:
//!
//! - `parse::build_expression` holds `expect`s that encode grammar invariants. Each is correct
//!   against today's `trestle.pest`; a grammar edit that makes a child optional turns one into
//!   a trap.
//! - `evaluate::eval_expr` and `render`'s formatters recurse without a depth limit, and the
//!   wasm stack is smaller than a native thread's — so deeply nested source overflows here
//!   sooner than it does under `cargo test`.
//!
//! Faults a *correct* program can reach are errors rather than panics: integer literals that do
//! not fit an `i64` are a `BuildError`, and division by zero and arithmetic overflow are
//! `EvalError`s. All three arrive in the editor as ordinary diagnostics.

mod diagnostics;
/// Public so `tests/wire_format.rs` can build one of each shape and generate the fixtures the
/// playground type-checks against. It is the wire format; nothing here is an implementation
/// detail.
pub mod dto;
mod render;

use miette::{LabeledSpan, MietteDiagnostic};
use pest::Parser as _;
use wasm_bindgen::prelude::*;

use trestle::AnalysisError;
use trestle::parse::ast::ParsedProgram;
use trestle::parse::{Rule, TrestleParser, build_program};
use trestle::prelude::prelude_span;
use trestle::type_check::TypeCheckedProgram;
use trestle::type_check::typed_ast::TypedBinding;

use crate::diagnostics::{LineIndex, from_miette};
use crate::dto::{Binding, CheckResult, Diagnostic, Phase};
use crate::render::{format_type, format_value};

#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
}

/// The version of this shim, so the playground can show which compiler build it loaded.
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}

/// Parse and analyse `source`, without evaluating.
#[wasm_bindgen]
pub fn check(source: &str) -> Result<JsValue, JsValue> {
    let index = LineIndex::new(source);

    let result = match analyse(source, &index) {
        Ok(program) => CheckResult::ok(bindings_of(&program, &index)),
        Err(diagnostics) => CheckResult::failed(diagnostics),
    };

    to_js(&result)
}

/// Parse, analyse and evaluate `source`.
#[wasm_bindgen]
pub fn run(source: &str) -> Result<JsValue, JsValue> {
    let index = LineIndex::new(source);

    let program = match analyse(source, &index) {
        Ok(program) => program,
        Err(diagnostics) => return to_js(&dto::RunResult::failed(diagnostics)),
    };

    // Both of these have to be read before evaluating, because `evaluate` takes the program
    // by value. The program's type is the type of its final expression.
    let bindings = bindings_of(&program, &index);
    let value_type = program
        .expressions
        .last()
        .map(|expression| format_type(&expression.ty))
        .unwrap_or_else(|| "Unit".to_owned());

    let result = match trestle::evaluate::evaluate(program) {
        Ok(value) => dto::RunResult::ok(format_value(&value), value_type, bindings),

        Err(error) => dto::RunResult::failed(vec![from_miette(error, Phase::Evaluate, &index)]),
    };

    to_js(&result)
}

/// Source text through to a type-checked program, collecting diagnostics from whichever
/// phase failed.
fn analyse(source: &str, index: &LineIndex<'_>) -> Result<TypeCheckedProgram, Vec<Diagnostic>> {
    let parsed = parse_source(source, index)?;

    trestle::analyse(parsed).map_err(|error| match error {
        // Both arms report the whole batch: Trestle fails fast between phases but collects
        // every error within one, and showing all of them at once is the point.
        AnalysisError::BindingResolution(errors) => errors
            .into_iter()
            .map(|error| from_miette(error, Phase::Resolve, index))
            .collect(),
        AnalysisError::TypeCheck(errors) => errors
            .into_iter()
            .map(|error| from_miette(error, Phase::Typecheck, index))
            .collect(),
    })
}

/// Deliberately *not* `trestle::parse`.
///
/// That function funnels a pest syntax error through `IntoDiagnostic`, which produces a
/// `miette::Report` carrying no labels — the byte offset pest knew about is gone, so the
/// editor has nothing to underline. Driving the two steps here keeps the structured pest
/// error, whose `location` we translate ourselves. The `build_program` path is unchanged:
/// `BuildError` already derives `Diagnostic` with real labels.
fn parse_source(source: &str, index: &LineIndex<'_>) -> Result<ParsedProgram, Vec<Diagnostic>> {
    let mut pairs = TrestleParser::parse(Rule::program, source)
        .map_err(|error| vec![from_pest(&error, index)])?;

    let program = pairs
        .next()
        .expect("the program rule always yields exactly one pair");

    build_program(program).map_err(|error| vec![from_miette(error, Phase::Parse, index)])
}

/// A pest parse failure, rebuilt as a diagnostic with a real span.
///
/// Assembled as a `miette::MietteDiagnostic` rather than by filling in the wire struct by hand,
/// so it goes through `from_miette` like every other error and picks up the graphical
/// rendering. pest is the one error source in the pipeline that is not already a
/// `miette::Diagnostic`; this is the adapter.
fn from_pest(error: &pest::error::Error<Rule>, index: &LineIndex<'_>) -> Diagnostic {
    let (offset, length) = match error.location {
        // A point failure — "expected X here". Widened to one character by `from_miette`,
        // which a zero-width squiggle would otherwise be invisible for.
        pest::error::InputLocation::Pos(offset) => (offset, 0),
        pest::error::InputLocation::Span((start, end)) => (start, end.saturating_sub(start)),
    };

    // `variant.message()` is the bare "expected …" text. `error.to_string()` would instead
    // be pest's multi-line rendering with its own caret art, which duplicates what miette is
    // about to draw.
    let diagnostic = MietteDiagnostic::new(error.variant.message().into_owned())
        .with_code("trestle::syntax_error")
        .with_label(LabeledSpan::new(
            Some("unexpected here".to_owned()),
            offset,
            length,
        ));

    from_miette(diagnostic, Phase::Parse, index)
}

fn bindings_of(program: &TypeCheckedProgram, index: &LineIndex<'_>) -> Vec<Binding> {
    program
        .bindings
        .iter()
        // Builtins are seeded with a zero-length span at offset 0 (see `prelude_span`).
        // They have no definition site in this source, so listing them would just be noise.
        .filter(|binding| binding.span != prelude_span())
        .map(|binding| binding_of(binding, index))
        .collect()
}

fn binding_of(binding: &TypedBinding, index: &LineIndex<'_>) -> Binding {
    let label = index.label(None, binding.span.offset(), binding.span.len().max(1));

    Binding {
        name: binding.name.clone(),
        ty: format_type(&binding.ty),
        start_line: label.start_line,
        start_column: label.start_column,
        end_line: label.end_line,
        end_column: label.end_column,
    }
}

/// `serialize_missing_as_null` rather than the default: without it a `None` field is simply
/// absent from the JS object, which contradicts the `string | null` the TypeScript side
/// declares. Emitting an explicit null keeps the two definitions honest about each other.
fn to_js<T: serde::Serialize>(value: &T) -> Result<JsValue, JsValue> {
    let serializer = serde_wasm_bindgen::Serializer::new().serialize_missing_as_null(true);

    value
        .serialize(&serializer)
        .map_err(|error| JsValue::from_str(&error.to_string()))
}
