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
/// Used for synthesized `Binary` nodes that span both operands.
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
pub enum Literal {
    Int(usize),
    Bool(bool),
    Float(f64),
    String(String),
}

/// A binary operator. The single source of truth for the operator set —
/// `resolved` and `analysed` reuse this rather than defining their own. Precedence
/// and associativity are not encoded here; they live in the `PrattParser`
/// (see `build_expression.rs`). Arithmetic ops take `Int`s and yield an `Int`;
/// comparison ops take `Int`s and yield a `Bool`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    And, // Bool × Bool → Bool
    Or,  // Bool × Bool → Bool
    Lt,
    Gt,
    Le,
    Ge,
    Eq,
    Neq,
    Pipe,
}

/// A prefix (unary) operator. Like [`BinaryOp`], the single source of truth reused by
/// `resolved` and `analysed`. `Neg` takes an `Int` and yields an `Int`; `Not` takes a
/// `Bool` and yields a `Bool`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Neg, // arithmetic negation: `-x`
    Not, // logical not: `!x`
}

#[derive(Debug, PartialEq)]
pub enum ExpressionKind {
    Literal(Literal),
    Var(String),
    Binary(BinaryOp, Box<Expression>, Box<Expression>),
    Unary(UnaryOp, Box<Expression>),
    Lambda(Lambda),
    FunctionInvocation {
        function_name: String,
        expressions: Vec<Expression>,
    },
    Let {
        name: String,
        type_dec: Option<TypeDeclaration>,
        value: Box<Expression>,
    },
    Block(Vec<Expression>),
    If {
        condition: Box<Expression>,
        true_pathway: Box<Expression>,
        false_pathway: Option<Box<Expression>>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum TypeDeclaration {
    Named(String), // "Int", "String" — grows into Generic/Record/Fn in later tiers
}

#[derive(Debug, PartialEq)]
pub struct Param {
    pub name: String,
    pub type_dec: TypeDeclaration,
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
