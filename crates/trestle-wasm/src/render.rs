//! Display formatting for `Value` and `Type`.
//!
//! Neither implements `Display` upstream — the conformance corpus snapshots `Debug`, which
//! is fine for `.snap` files but reads badly in a UI (`Int(15)` rather than `15`). These
//! formatters exist purely for the playground, so they live here rather than in `trestle`.

use std::fmt::Write as _;

use trestle::evaluate::Value;
use trestle::type_check::typed_ast::{Literal, Type};

pub fn format_value(value: &Value) -> String {
    let mut out = String::new();
    write_value(&mut out, value);
    out
}

/// Recursive because of records: a field holds a `Value`, so the nesting is unbounded.
/// Everything stays on one line — the playground renders this into a plain `<p>`, which would
/// collapse any indentation anyway.
fn write_value(out: &mut String, value: &Value) {
    match value {
        Value::Int(int) => {
            let _ = write!(out, "{int}");
        }
        Value::Bool(boolean) => {
            let _ = write!(out, "{boolean}");
        }
        // `{:?}` on an f64 keeps a trailing `.0`, which is what distinguishes a Float from
        // an Int on screen.
        Value::Float(float) => {
            let _ = write!(out, "{float:?}");
        }
        Value::String(string) => {
            let _ = write!(out, "{string:?}");
        }
        Value::Unit => out.push_str("unit"),
        // Spelled the way a record literal is written in source, so what comes back is
        // something you could paste into the editor. The one departure: fields print in
        // `BTreeMap` order — alphabetical, not source order — which is what the record *type*
        // does too, so the value and its type line up field for field.
        Value::Record(fields) if fields.is_empty() => out.push_str("{}"),
        Value::Record(fields) => {
            out.push_str("{ ");
            for (index, (name, field)) in fields.iter().enumerate() {
                if index > 0 {
                    out.push_str(", ");
                }
                let _ = write!(out, "{name}: ");
                write_value(out, field);
            }
            out.push_str(" }");
        }
        // A closure has no printable form — the lambda body is an AST and the captured
        // environment is a scope chain. Its type is the useful part, and it is reconstructible
        // from the lambda: `Fn(parameter.ty, body.ty)`, both already resolved by the type
        // checker's substitution pass. `run` reports the type of the *top-level* value
        // alongside this one, but a closure nested inside a record has no such companion.
        Value::Closure { lambda, .. } => {
            out.push_str("<closure: ");
            write_function_type(
                out,
                lambda.parameter.as_ref().map(|parameter| &parameter.ty),
                &lambda.body.ty,
            );
            out.push('>');
        }
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
            write_function_type(out, parameter.as_deref(), result);
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

/// The arrow itself, without the parentheses its *position* may call for — a closure prints its
/// type inside `<…>`, which already delimits it, so only [`write_type`]'s `Fn` arm needs those.
/// `Value::Closure` doesn't hold a `Type::Fn` to hand off, so this takes the two halves loose:
/// the lambda carries them as `parameter.ty` and `body.ty`.
fn write_function_type(out: &mut String, parameter: Option<&Type>, result: &Type) {
    match parameter {
        // A zero-parameter lambda: `() => 1`.
        None => out.push_str("()"),
        Some(parameter) => write_type(out, parameter, true),
    }
    out.push_str(" -> ");
    write_type(out, result, false);
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
    use std::rc::Rc;

    use trestle::binding_resolution::BindingId;
    use trestle::evaluate::{Environment, Value};
    use trestle::type_check::typed_ast::{
        ExpressionKind, Lambda, Literal, Param, Type, TypeCheckedExpression, TypeCheckedLiteral,
        TypeVarId,
    };

    use super::{format_type, format_value};

    fn int() -> Type {
        Type::Literal(Literal::Int)
    }

    /// A closure over `(parameter) => <body of type `result`>`. The body is a placeholder
    /// literal: rendering only reads its `ty`, never its shape.
    fn closure(parameter: Option<Type>, result: Type) -> Value {
        let body = TypeCheckedExpression {
            kind: ExpressionKind::Literal(TypeCheckedLiteral::Unit),
            span: (0usize, 0usize).into(),
            ty: result,
        };

        Value::Closure {
            lambda: Rc::new(Lambda {
                parameter: parameter.map(|ty| Param {
                    binding: BindingId(0),
                    ty,
                }),
                body: Box::new(body),
            }),
            env: Environment::empty(),
        }
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

    /// Also pins the field order: `x` was inserted first, but `BTreeMap` sorts by name.
    #[test]
    fn a_record_value_prints_its_fields_alphabetically() {
        let record = Value::Record(BTreeMap::from([
            ("x".to_owned(), Value::Int(1)),
            ("name".to_owned(), Value::String("Sam".to_owned())),
        ]));
        assert_eq!(format_value(&record), r#"{ name: "Sam", x: 1 }"#);
    }

    #[test]
    fn record_values_nest() {
        let inner = Value::Record(BTreeMap::from([("y".to_owned(), Value::Int(2))]));
        let outer = Value::Record(BTreeMap::from([("point".to_owned(), inner)]));
        assert_eq!(format_value(&outer), "{ point: { y: 2 } }");
    }

    /// The padded braces of the non-empty case would leave `{  }` here.
    #[test]
    fn an_empty_record_value_prints_without_padding() {
        assert_eq!(format_value(&Value::Record(BTreeMap::new())), "{}");
    }

    #[test]
    fn a_closure_prints_its_type() {
        assert_eq!(
            format_value(&closure(Some(int()), int())),
            "<closure: Int -> Int>"
        );
    }

    #[test]
    fn a_parameterless_closure_prints_empty_parentheses() {
        assert_eq!(format_value(&closure(None, int())), "<closure: () -> Int>");
    }

    /// A higher-order parameter still needs its parentheses inside `<…>`, even though the
    /// closure's own type does not.
    #[test]
    fn a_closure_parameter_is_parenthesised() {
        let parameter = Type::Fn(Some(Box::new(int())), Box::new(int()));
        assert_eq!(
            format_value(&closure(Some(parameter), int())),
            "<closure: (Int -> Int) -> Int>"
        );
    }

    /// The case the type on a closure exists for: nested, there is no companion type line.
    #[test]
    fn a_closure_inside_a_record_carries_its_type() {
        let record = Value::Record(BTreeMap::from([(
            "id".to_owned(),
            closure(Some(int()), int()),
        )]));
        assert_eq!(format_value(&record), "{ id: <closure: Int -> Int> }");
    }
}
