//! Walker for `Rule::statement` → [`Let`].

use pest::iterators::Pair;

use crate::Rule;

use super::build_expression::build_expr;
use super::{Let, get_bindings};

/// Build a `Let` from a `Rule::statement` pair (which wraps a `let_binding`).
pub(super) fn build_let(pair: Pair<Rule>) -> Let {
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
