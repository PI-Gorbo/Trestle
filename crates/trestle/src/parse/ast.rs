//! Trestle AST (Increment 1) and the walker that turns pest pairs into it.
//!
//! The grammar (`trestle.pest`) is a full parser, so we walk its parse tree
//! into these types. The walker is split by what it builds — see the
//! `build_program` and `build_expression` submodules (declared in the parent
//! `parse` module).

use miette::SourceSpan;
use pest::{Span, iterators::Pair};

use super::Rule;

pub fn source_span_from_pest_span(pest_span: Span) -> SourceSpan {
    (pest_span.start(), pest_span.end() - pest_span.start()).into()
}

/// Merge two spans into one covering from the start of `a` to the end of `b`.
///
/// Used for synthesized binary nodes (`Add`/`Mul`) that span both operands.
/// Assumes `a` starts at or before `b` (true for the left-to-right operand fold).
pub fn merge_spans(a: SourceSpan, b: SourceSpan) -> SourceSpan {
    let start = a.offset();
    (start, b.offset() + b.len() - start).into()
}

/// A source-spanned expression node: what the expression *is* (`kind`) plus
/// where it came from (`span`). Every node in the tree carries its own span so
/// diagnostics can point at any sub-expression.
#[derive(Debug, PartialEq)]
pub struct Expression {
    pub kind: ExpressionKind,
    pub span: SourceSpan,
}

#[derive(Debug, PartialEq)]
pub enum ExpressionKind {
    Int(usize),
    Var(String),
    Add(Box<Expression>, Box<Expression>),
    Mul(Box<Expression>, Box<Expression>),
    Lambda(Lambda),
    FunctionInvocation {
        function_name: String,
        expressions: Vec<Expression>,
    },
    Let {
        name: String,
        value: Box<Expression>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum TypeDeclaration {
    Named(String), // "Int", "String" — grows into Generic/Record/Fn in later tiers
}

#[derive(Debug, PartialEq)]
pub struct Param {
    pub name: String,
    pub type_dec: Option<TypeDeclaration>,
}

#[derive(Debug, PartialEq)]
pub struct Lambda {
    pub parameter: Option<Param>,
    pub return_type: Option<TypeDeclaration>,
    pub body: Box<Expression>,
}

#[derive(Debug, PartialEq)]
pub struct LoweredProgram {
    pub expressions: Vec<Expression>,
}

pub(super) fn get_bindings<'a>(pair: Pair<'a, Rule>, expect_message: &'a str) -> Pair<'a, Rule> {
    pair.into_inner().next().expect(expect_message)
}
