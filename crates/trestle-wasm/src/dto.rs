//! The wire format between the compiler and the browser.
//!
//! Every type here is mirrored by hand in `apps/demo/app/lib/compiler/types.ts`. Keep the
//! two in step: `serde` renames to camelCase so the JS side reads idiomatically, and the
//! result types are discriminated unions on `ok` so the frontend can match exhaustively.

use serde::Serialize;

/// Which pipeline stage produced a diagnostic. Worth surfacing because Trestle fails fast
/// per phase — a batch of errors is always from one stage, and knowing which one tells the
/// reader whether the program failed to parse, to resolve, or to type-check.
#[derive(Serialize, Clone, Copy, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    Parse,
    Resolve,
    Typecheck,
    Evaluate,
}

#[derive(Serialize, Clone, Copy, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Advice,
}

/// One highlighted range. `miette` diagnostics can carry several — `DuplicateBinding` has
/// two ("redeclared here" and "first declared here") — so this is a list, not a single span.
///
/// Line/column are 1-based and the column is in UTF-16 code units, because that is what
/// Monaco's `IRange` expects. `offset`/`length` keep the original byte span for debugging.
#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Label {
    pub message: Option<String>,
    pub start_line: u32,
    pub start_column: u32,
    pub end_line: u32,
    pub end_column: u32,
    pub offset: u32,
    pub length: u32,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    pub phase: Phase,
    pub severity: Severity,
    /// `miette`'s diagnostic code, e.g. `trestle::unbound_name`.
    pub code: Option<String>,
    pub message: String,
    pub help: Option<String>,
    pub labels: Vec<Label>,
}

/// A top-level binding and the type inference settled on for it. Lifted straight from
/// `TypeCheckedProgram::bindings`, which already carries name + type + definition span.
#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Binding {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: String,
    pub start_line: u32,
    pub start_column: u32,
    pub end_line: u32,
    pub end_column: u32,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CheckOk {
    /// Always `true`. Present so the JS side has a discriminant to match on.
    pub ok: bool,
    pub bindings: Vec<Binding>,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RunOk {
    /// Always `true`.
    pub ok: bool,
    /// The evaluated value, rendered for display (`15`, `"hello"`, `<closure>`).
    pub value: String,
    /// The type the checker inferred for the program's final expression.
    pub value_type: String,
    pub bindings: Vec<Binding>,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Failure {
    /// Always `false`.
    pub ok: bool,
    pub diagnostics: Vec<Diagnostic>,
}

/// `untagged` so the JS object is exactly `{ ok, bindings }` or `{ ok, diagnostics }` —
/// no serde wrapper key. The `ok` field is the discriminant.
#[derive(Serialize, Debug)]
#[serde(untagged)]
pub enum CheckResult {
    Ok(CheckOk),
    Failed(Failure),
}

#[derive(Serialize, Debug)]
#[serde(untagged)]
pub enum RunResult {
    Ok(RunOk),
    Failed(Failure),
}

impl CheckResult {
    pub fn ok(bindings: Vec<Binding>) -> Self {
        Self::Ok(CheckOk { ok: true, bindings })
    }

    pub fn failed(diagnostics: Vec<Diagnostic>) -> Self {
        Self::Failed(Failure {
            ok: false,
            diagnostics,
        })
    }
}

impl RunResult {
    pub fn ok(value: String, value_type: String, bindings: Vec<Binding>) -> Self {
        Self::Ok(RunOk {
            ok: true,
            value,
            value_type,
            bindings,
        })
    }

    pub fn failed(diagnostics: Vec<Diagnostic>) -> Self {
        Self::Failed(Failure {
            ok: false,
            diagnostics,
        })
    }
}
