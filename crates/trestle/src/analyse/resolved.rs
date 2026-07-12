//! The resolved AST: the LoweredAst ([`ast`](crate::parse::ast)) after **name resolution only**
//! ([`resolve_names`](super::resolve_names)), before type checking ([`type_check`](super::type_check)).
//!
//! It mirrors the *lowered* tree (not the analysed one), differing only where pass 1 changes a
//! field: every `String` name becomes a [`BindingId`], and each binding's name+span is recorded
//! in the side [`ResolvedProgram::bindings`] table (indexed by `BindingId`). Type annotations are
//! carried through untouched as [`ast::TypeDeclaration`] — pass 2 interprets them into
//! [`Type`](super::analysed::Type). No node carries a type yet. There is no `If` variant: the
//! grammar parses `if`, but its lowering (an `ast::If`, and arms here + in type-check) is deferred.

use miette::SourceSpan;

use super::analysed::BindingId;
use crate::parse::ast::{BinaryOp, TypeDeclaration};

/// A name-resolved, not-yet-typed expression: what it is (`kind`) and where it came from (`span`).
#[derive(Debug, PartialEq)]
pub struct ResolvedExpression {
    pub kind: ResolvedExpressionKind,
    pub span: SourceSpan,
}

#[derive(Debug, PartialEq)]
pub enum ResolvedLiteral {
    Int(usize),
    Bool(bool),
    Float(f64),
    String(String),
}

#[derive(Debug, PartialEq)]
pub enum ResolvedExpressionKind {
    Literal(ResolvedLiteral),
    Var(BindingId), // was Var(String)
    Binary(BinaryOp, Box<ResolvedExpression>, Box<ResolvedExpression>),
    Lambda(ResolvedLambda),
    FunctionInvocation(BindingId, Vec<ResolvedExpression>), // callee resolved; was String
    Let {
        binding: BindingId, // was name: String
        value: Box<ResolvedExpression>,
    },
    Block(Vec<ResolvedExpression>),
    If {
        condition: Box<ResolvedExpression>,
        true_condition: Box<ResolvedExpression>,
        false_condition: Option<Box<ResolvedExpression>>,
    },
}

#[derive(Debug, PartialEq)]
pub struct ResolvedParam {
    pub binding: BindingId,
    /// Raw annotation, still unresolved — pass 2 turns this into a [`Type`](super::analysed::Type).
    pub type_dec: TypeDeclaration,
}

#[derive(Debug, PartialEq)]
pub struct ResolvedLambda {
    pub parameter: Option<ResolvedParam>,
    pub return_type: Option<TypeDeclaration>,
    pub body: Box<ResolvedExpression>,
}

/// Name + definition site for each [`BindingId`]. Pass 2 pairs each with a computed type to
/// produce the analysed [`BindingInfo`](super::analysed::BindingInfo).
#[derive(Debug, PartialEq)]
pub struct ResolvedBinding {
    pub name: String,
    pub span: SourceSpan,
}

/// A name-resolved program: the resolved tree plus the binding table (indexed by `BindingId`).
#[derive(Debug, PartialEq)]
pub struct ResolvedProgram {
    pub expressions: Vec<ResolvedExpression>,
    pub bindings: Vec<ResolvedBinding>,
}
