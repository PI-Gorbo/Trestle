//! Trestle AST (Increment 1) and the walker that turns pest pairs into it.
//!
//! The grammar (`trestle.pest`) is a full parser, so we walk its parse tree
//! into these types. The walker is split by what it builds — see the
//! `build_program`, `build_statement`, and `build_expression` submodules.

use pest::iterators::Pair;

use crate::Rule;

mod build_expression;
mod build_program;
mod build_statement;

pub use build_program::build_program;

#[derive(Debug, PartialEq)]
pub enum Expr {
    Int(i64),
    Var(String),
    Add(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
    Lambda(Lambda),
}

#[derive(Debug, PartialEq)]
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
    pub params: Vec<Param>,
    pub return_type: Option<TypeDeclaration>,
    pub body: Box<Expr>,
}

#[derive(Debug, PartialEq)]
pub struct Let {
    pub name: String,
    pub value: Expr,
}

#[derive(Debug, PartialEq)]
pub struct Program {
    pub statements: Vec<Let>,
}

fn get_bindings<'a>(pair: Pair<'a, Rule>, expect_message: &'a str) -> Pair<'a, Rule> {
    pair.into_inner().next().expect(expect_message)
}
