//! The analysed AST: the LoweredAst ([`ast::LoweredProgram`](crate::parse::ast::LoweredProgram))
//! after name resolution and type checking. [`analyse`](crate::analyse::analyse) produces this;
//! [`evaluate`](crate::evaluate::evaluate) consumes it.
//!
//! It mirrors the [`ast`](crate::parse::ast) tree, differing only where analysis changes a
//! field: `String` names become [`BindingId`]s, and every [`Expression`] carries its
//! [`Type`]. Names and types per binding live in the side [`AnalysedProgram::bindings`]
//! table so the tree itself stays ids-only.

use miette::SourceSpan;

/// Index of a binding site (a `let` or a lambda parameter). Assigned during analysis;
/// indexes into [`AnalysedProgram::bindings`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BindingId(pub usize);

/// An analysed type. Concrete for now; add a `Var(TypeVarId)` variant here if you later
/// want inference for unannotated params (e.g. `(a, b) => a + b`).
#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    Unit,
    Literal(Literal),
    Fn(Option<Box<Type>>, Box<Type>), // curried: one arg -> result
}

#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    Int,
    Bool,
    Float,
    String,
}

/// Name + resolved type + definition site for each [`BindingId`].
#[derive(Debug, PartialEq)]
pub struct AnalysedBinding {
    pub name: String,
    pub ty: Type,
    pub span: SourceSpan,
}

/// An analysed expression node: what it is (`kind`), where it came from (`span`), and its
/// resolved type (`ty`, new vs the LoweredAst).
#[derive(Debug, PartialEq)]
pub struct AnalysedExpression {
    pub kind: ExpressionKind,
    pub span: SourceSpan,
    pub ty: Type,
}

#[derive(Debug, PartialEq)]
pub enum AnalysedLiteral {
    Int(usize),
    Bool(bool),
    Float(f64),
    String(String),
}

#[derive(Debug, PartialEq)]
pub enum ExpressionKind {
    Literal(AnalysedLiteral),
    Var(BindingId), // was Var(String)
    Add(Box<AnalysedExpression>, Box<AnalysedExpression>),
    Mul(Box<AnalysedExpression>, Box<AnalysedExpression>),
    If {
        condition: Box<AnalysedExpression>,
        then_branch: Box<AnalysedExpression>,
        else_branch: Box<AnalysedExpression>,
    },
    Lambda(Lambda),
    FunctionInvocation(BindingId, Vec<AnalysedExpression>), // callee resolved; was String
    Let {
        binding: BindingId, // was name: String
        value: Box<AnalysedExpression>,
    },
    Block(Vec<AnalysedExpression>),
}

#[derive(Debug, PartialEq)]
pub struct Param {
    pub binding: BindingId,
    pub ty: Type, // resolved; was Option<TypeDeclaration>
}

#[derive(Debug, PartialEq)]
pub struct Lambda {
    pub parameter: Option<Param>,
    pub body: Box<AnalysedExpression>,
    // return_type is gone: the lambda's type is Fn(param.ty, body.ty), held in Expression::ty.
}

/// A fully analysed program: the analysed tree plus the binding table it resolves against.
#[derive(Debug, PartialEq)]
pub struct AnalysedProgram {
    pub expressions: Vec<AnalysedExpression>,
    pub bindings: Vec<AnalysedBinding>,
}
