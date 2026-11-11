//! Trestle AST (Increment 1) and the walker that turns pest pairs into it.
//!
//! The grammar (`trestle.pest`) is a full parser, so we walk its parse tree
//! into these types. The walker is split by what it builds — see the
//! `build_program` and `build_expression` submodules (declared in the parent
//! `parse` module).

use std::collections::BTreeMap;

use miette::SourceSpan;
use pest::{Span, iterators::Pair};

use super::Rule;

pub fn source_span_from_pest_span(pest_span: Span) -> SourceSpan {
    (pest_span.start(), pest_span.end() - pest_span.start()).into()
}

pub fn merge_spans(a: SourceSpan, b: SourceSpan) -> SourceSpan {
    let start = a.offset();
    (start, b.offset() + b.len() - start).into()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptionallyNamedTypeExpression {
    pub type_dec: Box<TypeExpression>,
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LambdaTypeExpression {
    pub parameter: Option<OptionallyNamedTypeExpression>,
    pub return_type: Box<TypeExpression>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeExpressionKind {
    Named(String),
    Record(BTreeMap<String, TypeExpression>),
    Lambda(LambdaTypeExpression),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeExpression {
    pub kind: TypeExpressionKind,
    pub span: SourceSpan,
}

#[derive(Debug, PartialEq)]
pub struct Expression {
    pub kind: ExpressionKind,
    pub span: SourceSpan,
}

#[derive(Debug, PartialEq)]
pub enum Literal {
    Record(BTreeMap<String, Expression>),
    Int(i64),
    Bool(bool),
    Float(f64),
    String(String),
    Unit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    And,
    Or,
    Lt,
    Gt,
    Le,
    Ge,
    Eq,
    Neq,
    Pipe,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Neg,
    Not,
}

#[derive(Debug, PartialEq)]
pub enum ExpressionKind {
    Literal(Literal),
    Var(String),
    Binary(BinaryOp, Box<Expression>, Box<Expression>),
    Unary(UnaryOp, Box<Expression>),
    Lambda(Lambda),
    FunctionInvocation {
        function: Box<Expression>,
        arguments: Vec<Expression>,
    },
    FieldAccess {
        target: Box<Expression>,
        identifier: String,
    },
    Let {
        name: String,
        type_dec: Option<TypeExpression>,
        value: Box<Expression>,
    },
    Block(Vec<Expression>),
    If {
        condition: Box<Expression>,
        true_pathway: Box<Expression>,
        false_pathway: Option<Box<Expression>>,
    },
    TypeDeclaration {
        identifier: String,
        type_expression: TypeExpression,
    },
}

#[derive(Debug, PartialEq)]
pub struct Param {
    pub name: String,
    pub type_dec: Option<TypeExpression>,
}

#[derive(Debug, PartialEq)]
pub struct Lambda {
    pub parameter: Option<Param>,
    pub return_type: Option<TypeExpression>,
    pub body: Box<Expression>,
}

#[derive(Debug, PartialEq)]
pub struct ParsedProgram {
    pub expressions: Vec<Expression>,
}

pub(super) fn get_bindings<'a>(pair: Pair<'a, Rule>, expect_message: &'a str) -> Pair<'a, Rule> {
    pair.into_inner().next().expect(expect_message)
}
