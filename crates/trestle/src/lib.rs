//! Trestle language: the three compiler phases, each in its own folder.
//!
//! Source flows [`parse()`] → [`analyse::analyse`] → [`evaluate::evaluate`]. The binary
//! (`main.rs`) and the conformance suite (`trestle-tests` crate) drive the language
//! through these public entry points; [`parse()`] is re-exported here for convenience.

// Phase 1 — grammar, parser, and the parsed AST.
pub mod parse;
// Phase 2a — name resolution (parsed AST → binding-resolved AST).
pub mod binding_resolution;
// Phase 2b — type checking (binding-resolved AST → type-checked AST).
pub mod type_check;
// Phase 2 — orchestrates binding resolution + type checking into one linear pass.
pub mod analyse;
// Phase 3 — tree-walk evaluation.
pub mod evaluate;

pub use parse::parse;
