//! The type-checked AST: the binding-resolved AST
//! ([`BindingResolvedProgram`](crate::binding_resolution::BindingResolvedProgram)) after type
//! checking. [`analyse`](super::analyse) produces this; [`evaluate`](crate::evaluate::evaluate)
//! consumes it.
//!
//! It mirrors the binding-resolved tree, differing only where type checking changes a field: every
//! [`Expression`](TypeCheckedExpression) carries its [`Type`]. Names and types per binding live in
//! the side [`TypeCheckedProgram::bindings`] table so the tree itself stays ids-only.

use miette::SourceSpan;

use crate::binding_resolution::BindingId;
use crate::parse::ast::{BinaryOp, UnaryOp};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeVarId(pub usize);

#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    Unit,
    Literal(Literal),
    Var(TypeVarId),
    Fn(Option<Box<Type>>, Box<Type>), // curried: one arg -> result
}

#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    Int,
    Bool,
    Float,
    String,
    Unit,
}

/// Name + resolved type + definition site for each [`BindingId`].
#[derive(Debug, PartialEq)]
pub struct TypeCheckedBinding {
    pub name: String,
    pub ty: Type,
    pub span: SourceSpan,
}

/// A type-checked expression node: what it is (`kind`), where it came from (`span`), and its
/// resolved type (`ty`, new vs the binding-resolved AST).
#[derive(Debug, Clone, PartialEq)]
pub struct TypeCheckedExpression {
    pub kind: ExpressionKind,
    pub span: SourceSpan,
    pub ty: Type,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TypeCheckedLiteral {
    Int(usize),
    Bool(bool),
    Float(f64),
    String(String),
    Unit,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExpressionKind {
    Literal(TypeCheckedLiteral),
    Var(BindingId), // was Var(String)
    Binary(
        BinaryOp,
        Box<TypeCheckedExpression>,
        Box<TypeCheckedExpression>,
    ),
    Unary(UnaryOp, Box<TypeCheckedExpression>),
    If {
        condition: Box<TypeCheckedExpression>,
        then_branch: Box<TypeCheckedExpression>,
        else_branch: Option<Box<TypeCheckedExpression>>,
    },
    Lambda(Lambda),
    FunctionInvocation(BindingId, Vec<TypeCheckedExpression>), // callee resolved; was String
    Let {
        binding: BindingId, // was name: String
        value: Box<TypeCheckedExpression>,
    },
    Block(Vec<TypeCheckedExpression>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub binding: BindingId,
    pub ty: Type, // resolved; was Option<TypeDeclaration>
}

#[derive(Debug, Clone, PartialEq)]
pub struct Lambda {
    pub parameter: Option<Param>,
    pub body: Box<TypeCheckedExpression>,
    // return_type is gone: the lambda's type is Fn(param.ty, body.ty), held in Expression::ty.
}

/// A fully type-checked program: the typed tree plus the binding table it resolves against.
#[derive(Debug, PartialEq)]
pub struct TypeCheckedProgram {
    pub expressions: Vec<TypeCheckedExpression>,
    pub bindings: Vec<TypeCheckedBinding>,
}
