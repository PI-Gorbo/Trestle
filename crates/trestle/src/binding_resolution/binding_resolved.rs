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

use crate::parse::ast::{BinaryOp, UnaryOp};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeBindingId(pub usize);

/// [`Type`](crate::type_check::typed_ast::Type).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedTypeExpressionKind {
    Named(TypeBindingId),
    Record(BTreeMap<String, ResolvedTypeExpression>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedTypeExpression {
    pub kind: ResolvedTypeExpressionKind,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BindingId(pub usize);

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
    FunctionInvocation {
        function: Box<ResolvedExpression>,
        arguments: Vec<ResolvedExpression>,
    },
    FieldAccess {
        target: Box<ResolvedExpression>,
        field_name: String,
    },
    Let {
        binding: BindingId, // was name: String
        /// Raw annotation, still unresolved — type checking interprets it into a [`Type`](crate::type_check::typed_ast::Type).
        type_dec: Option<ResolvedTypeExpression>,
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
    pub type_dec: Option<ResolvedTypeExpression>,
}

#[derive(Debug, PartialEq)]
pub struct ResolvedLambda {
    pub parameter: Option<ResolvedParam>,
    pub return_type: Option<ResolvedTypeExpression>,
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
