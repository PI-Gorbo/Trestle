//! The checked AST: [`ast::Program`](crate::ast::Program) (the LoweredAst) after name
//! resolution and type checking. [`analyse`](crate::analysis::analyse) produces this;
//! [`evaluate`](crate::evaluation::evaluate) consumes it.
//!
//! It mirrors the [`ast`](crate::ast) tree, differing only where checking changes a
//! field: `String` names become [`BindingId`]s, and every [`Expression`] carries its
//! [`Type`]. Names and types per binding live in the side [`CheckedProgram::bindings`]
//! table so the tree itself stays ids-only.

use miette::SourceSpan;

/// Index of a binding site (a `let` or a lambda parameter). Assigned during analysis;
/// indexes into [`CheckedProgram::bindings`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BindingId(pub u32);

/// A checked type. Concrete for now; add a `Var(TypeVarId)` variant here if you later
/// want inference for unannotated params (e.g. `(a, b) => a + b`).
#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    Int,
    Fn(Box<Type>, Box<Type>), // curried: one arg -> result
}

/// Name + resolved type + definition site for each [`BindingId`].
#[derive(Debug, PartialEq)]
pub struct BindingInfo {
    pub name: String,
    pub ty: Type,
    pub span: SourceSpan,
}

/// A checked expression node: what it is (`kind`), where it came from (`span`), and its
/// resolved type (`ty`, new vs the LoweredAst).
#[derive(Debug, PartialEq)]
pub struct Expression {
    pub kind: ExpressionKind,
    pub span: SourceSpan,
    pub ty: Type,
}

#[derive(Debug, PartialEq)]
pub enum ExpressionKind {
    Int(i64),
    Var(BindingId), // was Var(String)
    Add(Box<Expression>, Box<Expression>),
    Mul(Box<Expression>, Box<Expression>),
    Lambda(Lambda),
    FunctionInvocation(BindingId, Vec<Expression>), // callee resolved; was String
    Let {
        binding: BindingId, // was name: String
        value: Box<Expression>,
    },
}

#[derive(Debug, PartialEq)]
pub struct Param {
    pub binding: BindingId,
    pub ty: Type, // resolved; was Option<TypeDeclaration>
}

#[derive(Debug, PartialEq)]
pub struct Lambda {
    pub parameter: Option<Param>,
    pub body: Box<Expression>,
    // return_type is gone: the lambda's type is Fn(param.ty, body.ty), held in Expression::ty.
}

/// A fully checked program: the checked tree plus the binding table it resolves against.
#[derive(Debug, PartialEq)]
pub struct CheckedProgram {
    pub expressions: Vec<Expression>,
    pub bindings: Vec<BindingInfo>, // indexed by BindingId.0
}
