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

use miette::SourceSpan;

use crate::parse::ast::{BinaryOp, TypeDeclaration, UnaryOp};

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
        type_dec: Option<TypeDeclaration>,
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
    /// Raw annotation, still unresolved — type checking turns this into a [`Type`](crate::type_check::typed_ast::Type).
    pub type_dec: Option<TypeDeclaration>,
}

#[derive(Debug, PartialEq)]
pub struct ResolvedLambda {
    pub parameter: Option<ResolvedParam>,
    pub return_type: Option<TypeDeclaration>,
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
}
