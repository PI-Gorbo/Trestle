//! Trestle language: the three compiler phases, each in its own folder.
//!
//! Source flows [`parse()`] → [`analyse::analyse`] → [`evaluate::evaluate`]. The binary
//! (`main.rs`) and the conformance suite (`trestle-tests` crate) drive the language
//! through these public entry points; [`parse()`] is re-exported here for convenience.

// Phase 1 — grammar, parser, and the lowered AST.
pub mod parse;
// Phase 2 — name resolution and type checking.
pub mod analyse;
// Phase 3 — tree-walk evaluation.
pub mod evaluate;

pub use parse::parse;
