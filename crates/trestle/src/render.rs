//! Display formatting for `Value` and `Type`.
//!
//! The conformance corpus snapshots `Debug`, which is fine for `.snap` files but reads badly
//! to a person (`Int(15)` rather than `15`). `Display` is what the CLI and the playground show.

use std::fmt::{self, Write as _};

use crate::evaluate::Value;
use crate::type_check::typed_ast::{Literal, RecordTail, Type, TypeVarId};

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&format_value(self))
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&format_type(self))
    }
}

fn format_value(value: &Value) -> String {
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
                &mut Vec::new(),
                lambda.parameter.as_ref().map(|parameter| &parameter.ty),
                &lambda.body.ty,
            );
            out.push('>');
        }
    }
}

fn format_type(ty: &Type) -> String {
    let mut out = String::new();
    write_type(&mut out, &mut Vec::new(), ty, false);
    out
}

/// "`A` occurs in `A -> B`": the variable and the type share one naming table, so the
/// variable's name is the one it has inside the type.
pub(crate) fn format_occurs(var: TypeVarId, ty: &Type) -> String {
    let mut names = Vec::new();
    let mut out = String::from("`");
    write_var(&mut out, &mut names, var);
    out.push_str("` occurs in `");
    write_type(&mut out, &mut names, ty, false);
    out.push('`');
    out
}

/// Names type variables `A`, `B`, … by order of first appearance in the type being printed
/// (`A1`, `B1`, … past `Z`). `names` holds the variables seen so far; an index into it is a name.
fn write_var(out: &mut String, names: &mut Vec<TypeVarId>, var: TypeVarId) {
    let index = names
        .iter()
        .position(|seen| *seen == var)
        .unwrap_or_else(|| {
            names.push(var);
            names.len() - 1
        });
    out.push(char::from(b'A' + (index % 26) as u8));
    if index >= 26 {
        let _ = write!(out, "{}", index / 26);
    }
}

/// `parenthesise_function` is set when `ty` sits in a position where a bare arrow would
/// re-associate: the left of another arrow. `->` is right-associative (currying is
/// desugared that way), so `Int -> Int -> Int` needs no parens but `(Int -> Int) -> Int`
/// does.
fn write_type(
    out: &mut String,
    names: &mut Vec<TypeVarId>,
    ty: &Type,
    parenthesise_function: bool,
) {
    match ty {
        Type::Unit => out.push_str("Unit"),
        Type::Literal(literal) => out.push_str(format_literal(literal)),
        // Any variable left after type checking is shown as a generic, generalised or not:
        // `TypedBinding` carries no scheme to tell the two apart.
        Type::Var(var) => write_var(out, names, *var),
        Type::Fn {
            parameter,
            return_type: result,
        } => {
            if parenthesise_function {
                out.push('(');
            }
            write_function_type(out, names, parameter.as_deref(), result);
            if parenthesise_function {
                out.push(')');
            }
        }
        Type::Record {
            fields,
            tail: RecordTail::Closed,
        } if fields.is_empty() => out.push_str("{}"),
        // An open tail prints as `..A` after the fields: "these, plus whatever row `A` holds".
        Type::Record { fields, tail } => {
            out.push_str("{ ");
            for (index, (name, field)) in fields.iter().enumerate() {
                if index > 0 {
                    out.push_str(", ");
                }
                let _ = write!(out, "{name}: ");
                write_type(out, names, field, false);
            }
            if let RecordTail::Open(row) = tail {
                if !fields.is_empty() {
                    out.push_str(", ");
                }
                out.push_str("...");
                write_var(out, names, *row);
            }
            out.push_str(" }");
        }
    }
}

/// The arrow itself, without the parentheses its *position* may call for — a closure prints its
/// type inside `<…>`, which already delimits it, so only [`write_type`]'s `Fn` arm needs those.
/// `Value::Closure` doesn't hold a `Type::Fn` to hand off, so this takes the two halves loose:
/// the lambda carries them as `parameter.ty` and `body.ty`.
fn write_function_type(
    out: &mut String,
    names: &mut Vec<TypeVarId>,
    parameter: Option<&Type>,
    result: &Type,
) {
    match parameter {
        // A zero-parameter lambda: `() => 1`.
        None => out.push_str("()"),
        Some(parameter) => write_type(out, names, parameter, true),
    }
    out.push_str(" -> ");
    write_type(out, names, result, false);
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

    use crate::binding_resolution::BindingId;
    use crate::evaluate::{Environment, Value};
    use crate::type_check::typed_ast::{
        ExpressionKind, Lambda, Literal, Param, RecordTail, Type, TypeCheckedExpression,
        TypeCheckedLiteral, TypeVarId,
    };

    use super::{format_occurs, format_type, format_value};

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
            self_binding: None,
        }
    }

    #[test]
    fn curried_functions_need_no_parentheses() {
        let curried = Type::Fn {
            parameter: Some(Box::new(int())),
            return_type: Box::new(Type::Fn {
                parameter: Some(Box::new(int())),
                return_type: Box::new(int()),
            }),
        };
        assert_eq!(format_type(&curried), "Int -> Int -> Int");
    }

    #[test]
    fn a_function_parameter_is_parenthesised() {
        let higher_order = Type::Fn {
            parameter: Some(Box::new(Type::Fn {
                parameter: Some(Box::new(int())),
                return_type: Box::new(int()),
            })),
            return_type: Box::new(int()),
        };
        assert_eq!(format_type(&higher_order), "(Int -> Int) -> Int");
    }

    #[test]
    fn a_parameterless_function_prints_empty_parentheses() {
        assert_eq!(
            format_type(&Type::Fn {
                parameter: None,
                return_type: Box::new(int())
            }),
            "() -> Int"
        );
    }

    #[test]
    fn type_variables_are_named_in_order_of_appearance() {
        let ty = Type::Fn {
            parameter: Some(Box::new(Type::Var(TypeVarId(7)))),
            return_type: Box::new(Type::Fn {
                parameter: Some(Box::new(Type::Var(TypeVarId(3)))),
                return_type: Box::new(Type::Var(TypeVarId(7))),
            }),
        };
        assert_eq!(format_type(&ty), "A -> B -> A");
    }

    /// The variable takes its name from the same table as the type, so the two line up.
    #[test]
    fn an_occurs_message_names_the_variable_as_the_type_does() {
        let ty = Type::Fn {
            parameter: Some(Box::new(Type::Var(TypeVarId(7)))),
            return_type: Box::new(Type::Var(TypeVarId(3))),
        };
        assert_eq!(format_occurs(TypeVarId(7), &ty), "`A` occurs in `A -> B`");
    }

    #[test]
    fn records_print_their_fields() {
        let mut fields = BTreeMap::new();
        fields.insert("x".to_owned(), Box::new(int()));
        fields.insert("y".to_owned(), Box::new(int()));
        assert_eq!(
            format_type(&Type::Record {
                fields,
                tail: RecordTail::Closed
            }),
            "{ x: Int, y: Int }"
        );
    }

    #[test]
    fn an_empty_closed_record_type_prints_without_padding() {
        let ty = Type::Record {
            fields: BTreeMap::new(),
            tail: RecordTail::Closed,
        };
        assert_eq!(format_type(&ty), "{}");
    }

    /// The row variable shares its naming with the field variables.
    #[test]
    fn an_open_record_prints_its_row_after_the_fields() {
        let ty = Type::Record {
            fields: BTreeMap::from([("x".to_owned(), Box::new(Type::Var(TypeVarId(5))))]),
            tail: RecordTail::Open(TypeVarId(2)),
        };
        assert_eq!(format_type(&ty), "{ x: A, ...B }");
    }

    #[test]
    fn an_open_record_without_fields_prints_only_its_row() {
        let ty = Type::Record {
            fields: BTreeMap::new(),
            tail: RecordTail::Open(TypeVarId(2)),
        };
        assert_eq!(format_type(&ty), "{ ...A }");
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
        let parameter = Type::Fn {
            parameter: Some(Box::new(int())),
            return_type: Box::new(int()),
        };
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
