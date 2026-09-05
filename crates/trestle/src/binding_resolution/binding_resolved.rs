use std::collections::BTreeMap;

use miette::SourceSpan;

use crate::parse::ast::{BinaryOp, UnaryOp};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeBindingId(pub usize);


#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedOptionallyNamedTypeExpression {
    pub type_dec: Box<ResolvedTypeExpression>,
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedLambdaTypeExpression {
    pub parameter: Option<ResolvedOptionallyNamedTypeExpression>,
    pub return_type: Box<ResolvedTypeExpression>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedTypeExpressionKind {
    Named(TypeBindingId),
    Record(BTreeMap<String, ResolvedTypeExpression>),
    Lambda(ResolvedLambdaTypeExpression),
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
    Int(i64),
    Bool(bool),
    Float(f64),
    String(String),
    Record(BTreeMap<String, ResolvedExpression>),
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
    pub type_dec: Option<ResolvedTypeExpression>,
}

#[derive(Debug, PartialEq)]
pub struct ResolvedLambda {
    pub parameter: Option<ResolvedParam>,
    pub return_type: Option<ResolvedTypeExpression>,
    pub body: Box<ResolvedExpression>,
}

#[derive(Debug, PartialEq)]
pub struct ResolvedBinding {
    pub name: String,
    pub span: SourceSpan,
}

#[derive(Debug, PartialEq)]
pub struct BindingResolvedProgram {
    pub expressions: Vec<ResolvedExpression>,
    pub bindings: Vec<ResolvedBinding>,
    pub type_bindings: Vec<ResolvedBinding>,
}
