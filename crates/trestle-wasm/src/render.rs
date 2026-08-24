//! Display formatting for `Value` and `Type`.
//!
//! Neither implements `Display` upstream — the conformance corpus snapshots `Debug`, which
//! is fine for `.snap` files but reads badly in a UI (`Int(15)` rather than `15`). These
//! formatters exist purely for the playground, so they live here rather than in `trestle`.

use std::fmt::Write as _;

use trestle::evaluate::Value;
use trestle::type_check::typed_ast::{Literal, Type};

pub fn format_value(value: &Value) -> String {
    match value {
        Value::Int(int) => int.to_string(),
        Value::Bool(boolean) => boolean.to_string(),
        // `{:?}` on an f64 keeps a trailing `.0`, which is what distinguishes a Float from
        // an Int on screen.
        Value::Float(float) => format!("{float:?}"),
        Value::String(string) => format!("{string:?}"),
        Value::Unit => "unit".to_owned(),
        // A closure has no printable form — the lambda body is an AST and the captured
        // environment is a scope chain. Its *type* carries the useful information, and the
        // run result reports that alongside this.
        Value::Closure { .. } => "<closure>".to_owned(),
    }
}

pub fn format_type(ty: &Type) -> String {
    let mut out = String::new();
    write_type(&mut out, ty, false);
    out
}

/// `parenthesise_function` is set when `ty` sits in a position where a bare arrow would
/// re-associate: the left of another arrow. `->` is right-associative (currying is
/// desugared that way), so `Int -> Int -> Int` needs no parens but `(Int -> Int) -> Int`
/// does.
fn write_type(out: &mut String, ty: &Type, parenthesise_function: bool) {
    match ty {
        Type::Unit => out.push_str("Unit"),
        Type::Literal(literal) => out.push_str(format_literal(literal)),
        // Matches the spelling `TypeCheckError::InfiniteType` uses for an unsolved variable.
        Type::Var(var) => {
            let _ = write!(out, "_{}", var.0);
        }
        Type::Fn(parameter, result) => {
            if parenthesise_function {
                out.push('(');
            }
            match parameter {
                // A zero-parameter lambda: `() => 1`.
                None => out.push_str("()"),
                Some(parameter) => write_type(out, parameter, true),
            }
            out.push_str(" -> ");
            write_type(out, result, false);
            if parenthesise_function {
                out.push(')');
            }
        }
        Type::Record(fields) => {
            out.push_str("{ ");
            for (index, (name, field)) in fields.iter().enumerate() {
                if index > 0 {
                    out.push_str(", ");
                }
                let _ = write!(out, "{name}: ");
                write_type(out, field, false);
            }
            out.push_str(" }");
        }
    }
}

fn format_literal(literal: &Literal) -> &'static str {
    match literal {
        Literal::Int => "Int",
        Literal::Bool => "Bool",
        Literal::Float => "Float",
        Literal::String => "String",
        Literal::Unit => "Unit",
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use trestle::type_check::typed_ast::{Literal, Type, TypeVarId};

    use super::format_type;

    fn int() -> Type {
        Type::Literal(Literal::Int)
    }

    #[test]
    fn curried_functions_need_no_parentheses() {
        let curried = Type::Fn(
            Some(Box::new(int())),
            Box::new(Type::Fn(Some(Box::new(int())), Box::new(int()))),
        );
        assert_eq!(format_type(&curried), "Int -> Int -> Int");
    }

    #[test]
    fn a_function_parameter_is_parenthesised() {
        let higher_order = Type::Fn(
            Some(Box::new(Type::Fn(Some(Box::new(int())), Box::new(int())))),
            Box::new(int()),
        );
        assert_eq!(format_type(&higher_order), "(Int -> Int) -> Int");
    }

    #[test]
    fn a_parameterless_function_prints_empty_parentheses() {
        assert_eq!(format_type(&Type::Fn(None, Box::new(int()))), "() -> Int");
    }

    #[test]
    fn unsolved_variables_print_as_underscore_numbers() {
        assert_eq!(format_type(&Type::Var(TypeVarId(3))), "_3");
    }

    #[test]
    fn records_print_their_fields() {
        let mut fields = BTreeMap::new();
        fields.insert("x".to_owned(), Box::new(int()));
        fields.insert("y".to_owned(), Box::new(int()));
        assert_eq!(format_type(&Type::Record(fields)), "{ x: Int, y: Int }");
    }
}
