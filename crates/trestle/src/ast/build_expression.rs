//! Walkers for `Rule::expr` and everything nested under it: lambdas, type
//! declarations, and the precedence-encoded arithmetic levels.
//!
//! Precedence is encoded by the grammar's rule nesting (`add` < `mul`), so
//! each level only has to left-fold its children.

use pest::iterators::Pair;

use crate::Rule;

use super::{Expr, Lambda, Param, TypeDeclaration, get_bindings};

pub fn build_expr(pair: Pair<Rule>) -> Expr {
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
    Param { name, type_dec: ty }
}

/// `optional_type_declaration = ${ type_declaration? }`.
fn build_type_opt(pair: Pair<Rule>) -> Option<TypeDeclaration> {
    pair.into_inner().next().map(build_type)
}

/// `type_declaration = ${ ":" ~ identifier }` — inner yields the type name identifier.
fn build_type(pair: Pair<Rule>) -> TypeDeclaration {
    let ident = pair
        .into_inner()
        .next()
        .expect("type_declaration has an identifier");
    TypeDeclaration::Named(ident.as_str().to_string())
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
