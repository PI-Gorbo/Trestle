//! The binding-resolved AST: the parsed AST ([`ast`](crate::parse::ast)) after **name resolution
//! only** ([`resolve`](super::resolve)), before type checking ([`analyse`](crate::type_check::analyse)).
//!
//! It mirrors the *parsed* tree (not the type-checked one), differing only where resolution changes
//! a field: every `String` name becomes a [`BindingId`], and each binding's name+span is recorded
//! in the side [`BindingResolvedProgram::bindings`] table (indexed by `BindingId`). Type annotations
//! are carried through untouched as [`ast::TypeDeclaration`] — type checking interprets them into
//! [`Type`](crate::type_check::typed_ast::Type). No node carries a type yet. There is no `If`
//! variant: the grammar parses `if`, but its lowering (an `ast::If`, and arms here + in type-check)
//! is deferred.

use std::collections::BTreeMap;

use miette::SourceSpan;

use crate::{
    parse::ast::{BinaryOp, TypeExpression, UnaryOp},
    type_check::typed_ast::TypeVarId,
};

/// Index of a type declaration site (`type X = …`). The type-namespace twin of [`BindingId`]: type
/// names live in a namespace of their own, so `type Point = …` and `let Point = …` never collide.
/// No table is indexed by it yet — the declaration arm that mints these does not exist.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeBindingId(pub usize);

/// A name-resolved type annotation: the twin of
/// [`ast::TypeExpression`](crate::parse::ast::TypeExpression), differing only in `Named`, whose
/// `String` becomes a [`TypeBindingId`] — the same substitution `Var(String)` → `Var(BindingId)`
/// makes on the value side. Still not a [`Type`](crate::type_check::typed_ast::Type): resolution
/// says *which declaration* a type name refers to, never what it means.
///
/// Builtins (`Int`, `Bool`, `Float`, `String`, `Unit`) get no special variant. They resolve like
/// any other alias once the empty scope pre-seeds type bindings for them, which keeps this pass
/// free of type logic — it only ever knows *names*, and type checking maps the ids back to a
/// [`Type`](crate::type_check::typed_ast::Type).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedTypeExpression {
    Named(TypeBindingId), // was Named(String)
    /// Field order is not significant to a record type, so fields are keyed by name. A `BTreeMap`
    /// (not a `HashMap`) keeps the `Debug` rendering in a stable, name-sorted order — the corpus
    /// snapshots depend on it.
    Record(BTreeMap<String, ResolvedTypeExpression>),
}

// Nothing produces a `ResolvedTypeExpression` yet — that starts with `resolve_type_expression` and
// the `TypeDeclaration` arm. Until those land (together with the type-check side that reads the
// ids), the three annotation fields below — `Let::type_dec`, `ResolvedParam::type_dec` and
// `ResolvedLambda::return_type` — keep holding the raw `ast::TypeExpression`.

/// Index of a binding site (a `let` or a lambda parameter). Assigned during binding resolution;
/// indexes into [`BindingResolvedProgram::bindings`] and, after type checking, into
/// [`TypeCheckedProgram::bindings`](crate::type_check::typed_ast::TypeCheckedProgram).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BindingId(pub usize);

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
    Unit,
}

#[derive(Debug, PartialEq)]
pub enum ResolvedExpressionKind {
    Literal(ResolvedLiteral),
    Var(BindingId), // was Var(String)
    Binary(BinaryOp, Box<ResolvedExpression>, Box<ResolvedExpression>),
    Unary(UnaryOp, Box<ResolvedExpression>),
    Lambda(ResolvedLambda),
    FunctionInvocation(BindingId, Vec<ResolvedExpression>), // callee resolved; was String
    Let {
        binding: BindingId, // was name: String
        /// Raw annotation, still unresolved — type checking interprets it into a [`Type`](crate::type_check::typed_ast::Type).
        type_dec: Option<TypeExpression>,
        value: Box<ResolvedExpression>,
    },
    Block(Vec<ResolvedExpression>),
    If {
        condition: Box<ResolvedExpression>,
        true_condition: Box<ResolvedExpression>,
        false_condition: Option<Box<ResolvedExpression>>,
    },
    TypeDeclaration {
        identifier: TypeBindingId,
        type_expression: ResolvedTypeExpression,
    },
}

#[derive(Debug, PartialEq)]
pub struct ResolvedParam {
    pub binding: BindingId,
    /// Raw annotation, still unresolved — type checking turns this into a [`Type`](crate::type_check::typed_ast::Type).
    pub type_dec: Option<TypeExpression>,
}

#[derive(Debug, PartialEq)]
pub struct ResolvedLambda {
    pub parameter: Option<ResolvedParam>,
    pub return_type: Option<TypeExpression>,
    pub body: Box<ResolvedExpression>,
}

/// Name + definition site for each [`BindingId`]. Type checking pairs each with a computed type to
/// produce the [`TypeCheckedBinding`](crate::type_check::typed_ast::TypeCheckedBinding).
#[derive(Debug, PartialEq)]
pub struct ResolvedBinding {
    pub name: String,
    pub span: SourceSpan,
}

/// A name-resolved program: the resolved tree plus the binding table (indexed by `BindingId`).
#[derive(Debug, PartialEq)]
pub struct BindingResolvedProgram {
    pub expressions: Vec<ResolvedExpression>,
    pub bindings: Vec<ResolvedBinding>,
    pub type_bindings: Vec<ResolvedBinding>,
}
