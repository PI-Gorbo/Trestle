//! Trestle AST (Increment 1) and the walker that turns pest pairs into it.
//!
//! The grammar (`trestle.pest`) is a full parser, so we walk its parse tree
//! into these types. Precedence is encoded by the grammar's rule nesting
//! (`add` < `mul`), so the walker only has to left-fold each level.

use pest::iterators::Pair;

use crate::Rule;

#[derive(Debug, PartialEq)]
pub enum Expr {
    Int(i64),
    Var(String),
    Add(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
    Lambda(Lambda),
}

#[derive(Debug, PartialEq)]
pub enum Type {
    Named(String), // "Int", "String" — grows into Generic/Record/Fn in later tiers
}

#[derive(Debug, PartialEq)]
pub struct Param {
    pub name: String,
    pub ty: Option<Type>,
}

#[derive(Debug, PartialEq)]
pub struct Lambda {
    pub params: Vec<Param>,
    pub return_type: Option<Type>,
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

/// Build a `Program` from a `Rule::program` pair.
pub fn build_program(pair: Pair<Rule>) -> Program {
    Program {
        statements: pair.into_inner().fold(Vec::new(), |mut statements, pair| {
            match pair.as_rule() {
                Rule::statement => statements.push(build_let(pair)),
                Rule::EOI => {}
                rule => unreachable!("unexpected rule in program: {:?}", rule),
            }

            statements
        }),
    }
}

fn get_bindings<'a>(pair: Pair<'a, Rule>, expect_message: &'a str) -> Pair<'a, Rule> {
    pair.into_inner().next().expect(expect_message)
}

/// Build a `Let` from a `Rule::statement` pair (which wraps a `let_binding`).
fn build_let(pair: Pair<Rule>) -> Let {
    let let_binding = get_bindings(pair, "statement to have a let binding");

    let (name, value) =
        let_binding
            .into_inner()
            .fold((String::new(), None), |(mut name, mut value), p| {
                match p.as_rule() {
                    Rule::let_kw => {}
                    Rule::identifier_with_optional_type_declaration => {
                        name = p.as_str().to_string()
                    }
                    Rule::expr => value = Some(build_expr(p)),
                    rule => unreachable!("unexpected rule in let_binding: {:?}", rule),
                }
                (name, value)
            });

    Let {
        name,
        value: value.expect("let_binding has an expr"),
    }
}

fn build_expr(pair: Pair<Rule>) -> Expr {
    let expr_binding = get_bindings(pair, "expression to have bindings");
    match expr_binding.as_rule() {
        Rule::lambda => build_lambda(expr_binding),
        Rule::add => build_add(expr_binding),
        rule => unreachable!("unexpected rule in build_exp: {:?}", rule),
    }
}

fn build_lambda(pair: Pair<Rule>) -> Expr {
    let mut params = Vec::new();
    let mut return_type = None;
    let mut body = None;
    for p in pair.into_inner() {
        match p.as_rule() {
            Rule::identifier_with_optional_type_declaration => params.push(build_param(p)),
            Rule::optional_type_declaration => return_type = build_type_opt(p),
            Rule::expr => body = Some(build_expr(p)),
            rule => unreachable!("unexpected rule in lambda: {:?}", rule),
        }
    }
    Expr::Lambda(Lambda {
        params,
        return_type,
        body: Box::new(body.expect("lambda has a body")),
    })
}

/// `identifier_with_optional_type_declaration = ${ identifier ~ type_declaration? }`.
fn build_param(pair: Pair<Rule>) -> Param {
    let mut name = String::new();
    let mut ty = None;
    for p in pair.into_inner() {
        match p.as_rule() {
            Rule::identifier => name = p.as_str().to_string(),
            Rule::type_declaration => ty = Some(build_type(p)),
            rule => unreachable!("unexpected rule in param: {:?}", rule),
        }
    }
    Param { name, ty }
}

/// `optional_type_declaration = ${ type_declaration? }`.
fn build_type_opt(pair: Pair<Rule>) -> Option<Type> {
    pair.into_inner().next().map(build_type)
}

/// `type_declaration = ${ ":" ~ identifier }` — inner yields the type name identifier.
fn build_type(pair: Pair<Rule>) -> Type {
    let ident = pair
        .into_inner()
        .next()
        .expect("type_declaration has an identifier");
    Type::Named(ident.as_str().to_string())
}

/// `add = { mul ~ ("+" ~ mul)* }` — left-fold the `mul` children into `Add`.
fn build_add(pair: Pair<Rule>) -> Expr {
    let mut inner = pair.into_inner();
    let mut acc = build_mul(inner.next().expect("add has at least one mul"));
    for m in inner {
        acc = Expr::Add(Box::new(acc), Box::new(build_mul(m)));
    }
    acc
}

/// `mul = { primary ~ ("*" ~ primary)* }` — left-fold the `primary` children into `Mul`.
fn build_mul(pair: Pair<Rule>) -> Expr {
    let mut inner = pair.into_inner();
    let mut acc = build_primary(inner.next().expect("mul has at least one primary"));
    for p in inner {
        acc = Expr::Mul(Box::new(acc), Box::new(build_primary(p)));
    }
    acc
}

/// `primary = { int | ident | "(" ~ expr ~ ")" }`.
fn build_primary(pair: Pair<Rule>) -> Expr {
    let child = pair.into_inner().next().expect("primary has one child");
    match child.as_rule() {
        Rule::int => Expr::Int(child.as_str().parse().expect("int literal fits in i64")),
        Rule::identifier => Expr::Var(child.as_str().to_string()),
        Rule::expr => build_expr(child), // parenthesized expression
        rule => unreachable!("unexpected rule in primary: {:?}", rule),
    }
}
