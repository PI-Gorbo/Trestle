mod error;

pub use error::EvalError;

use std::collections::BTreeMap;
use std::rc::Rc;

use miette::SourceSpan;

use crate::binding_resolution::BindingId;
use crate::parse::ast::{BinaryOp, UnaryOp};
use crate::type_check::typed_ast::{
    self, ExpressionKind, TypeCheckedExpression, TypeCheckedLiteral, TypeCheckedProgram,
};

#[derive(Debug, Clone)]
pub enum Value {
    Int(i64),
    Bool(bool),
    Float(f64),
    String(String),
    Record(BTreeMap<String, Value>),
    Unit,
    Closure {
        lambda: Rc<typed_ast::Lambda>,
        env: Environment,
    },
}

#[derive(Debug, Clone, Default)]
pub struct Environment(Option<Rc<Scope>>);

#[derive(Debug)]
struct Scope {
    id: BindingId,
    value: Value,
    parent: Option<Rc<Scope>>,
}

impl Environment {
    pub fn empty() -> Self {
        Self(None)
    }

    pub fn extend(&self, id: BindingId, value: Value) -> Self {
        Self(Some(Rc::new(Scope {
            id,
            value,
            parent: self.0.clone(),
        })))
    }

    pub fn lookup(&self, id: BindingId) -> Option<&Value> {
        let mut current = self.0.as_deref();
        while let Some(scope) = current {
            if scope.id == id {
                return Some(&scope.value);
            }
            current = scope.parent.as_deref();
        }
        None
    }
}

pub fn evaluate(program: TypeCheckedProgram) -> Result<Value, EvalError> {
    program
        .expressions
        .iter()
        .try_fold((Environment::empty(), Value::Unit), |(mut env, _), expr| {
            let value = eval_expr(&mut env, expr)?;

            Ok((env, value))
        })
        .map(|(_, value)| value)
}

fn eval_expr(env: &mut Environment, expr: &TypeCheckedExpression) -> Result<Value, EvalError> {
    match &expr.kind {
        ExpressionKind::Literal(literal) => Ok(match literal {
            TypeCheckedLiteral::Int(value) => Value::Int(*value),
            TypeCheckedLiteral::Bool(value) => Value::Bool(*value),
            TypeCheckedLiteral::Float(value) => Value::Float(*value),
            TypeCheckedLiteral::String(value) => Value::String(value.clone()),
            TypeCheckedLiteral::Unit => Value::Unit,
            TypeCheckedLiteral::Record(fields) => Value::Record(
                fields
                    .iter()
                    .map(|(name, field)| Ok((name.clone(), eval_expr(env, field)?)))
                    .collect::<Result<BTreeMap<_, _>, EvalError>>()?,
            ),
        }),

        ExpressionKind::Var(id) => Ok(env
            .lookup(*id)
            .ok_or_else(|| {
                EvalError::invariant_due_to_type_check(
                    expr.span,
                    "resolved variable is not bound in the environment",
                )
            })?
            .clone()),

        ExpressionKind::Binary(op, lhs, rhs) => {
            let lhs = eval_expr(env, lhs)?;
            let rhs = eval_expr(env, rhs)?;
            eval_binary(*op, lhs, rhs, expr.span)
        }

        ExpressionKind::Let { binding, value } => {
            let bound = eval_expr(env, value)?;
            *env = env.extend(*binding, bound);
            Ok(Value::Unit)
        }

        ExpressionKind::Unary(op, operand) => {
            let evaluated_operand = eval_expr(env, operand)?;
            eval_unary(*op, evaluated_operand, operand.span)
        }

        ExpressionKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            let Value::Bool(taken) = eval_expr(env, condition)? else {
                return Err(EvalError::invariant_due_to_type_check(
                    condition.span,
                    "if condition did not evaluate to a Bool",
                ));
            };
            match (taken, else_branch) {
                (true, _) => eval_expr(env, then_branch),
                (false, Some(else_branch)) => eval_expr(env, else_branch),
                (false, None) => Ok(Value::Unit),
            }
        }

        ExpressionKind::Lambda(lambda) => Ok(Value::Closure {
            lambda: Rc::new(lambda.clone()),
            env: env.clone(),
        }),

        ExpressionKind::FunctionInvocation {
            function,
            arguments,
        } => {
            let evaluated_funciton = eval_expr(env, function)?;

            if arguments.is_empty() {
                if let Value::Closure { lambda, .. } = &evaluated_funciton {
                    if lambda.parameter.is_none() {
                        return Ok(apply(evaluated_funciton, None, function.span)?);
                    }
                }
            }

            let applied_function_result =
                arguments
                    .iter()
                    .try_fold(evaluated_funciton, |evaluated_funciton, arg| {
                        let evaluated_arg = eval_expr(env, arg)?;
                        Ok(apply(
                            evaluated_funciton,
                            Some(evaluated_arg),
                            function.span,
                        )?)
                    });

            applied_function_result
        }

        ExpressionKind::Block(exprs) => {
            let mut inner = env.clone();
            let mut result = Value::Unit;
            for expr in exprs {
                result = eval_expr(&mut inner, expr)?;
            }

            Ok(result)
        }

        ExpressionKind::FieldAccess { field_name, target } => {
            let evaluated_target = eval_expr(env, target)?;
            let Value::Record(record_inner) = evaluated_target else {
                return Err(EvalError::invariant_due_to_type_check(
                    target.span,
                    "field access target did not evaluate to a record",
                ));
            };

            record_inner.get(field_name).cloned().ok_or_else(|| {
                EvalError::invariant_due_to_type_check(
                    expr.span,
                    "record is missing a type-checked field",
                )
            })
        }

        ExpressionKind::TypeDeclaration {
            identifier: _,
            type_expression: _,
        } => Ok(Value::Unit),
    }
}

fn apply(closure: Value, arg: Option<Value>, span: SourceSpan) -> Result<Value, EvalError> {
    let Value::Closure { lambda, env } = closure else {
        return Err(EvalError::invariant_due_to_type_check(
            span,
            "callee did not evaluate to a closure",
        ));
    };
    let mut env = match (arg, &lambda.parameter) {
        (Some(arg), Some(param)) => env.extend(param.binding, arg),
        (None, None) => env,
        _ => {
            return Err(EvalError::invariant_due_to_type_check(
                span,
                "argument count does not match the closure's parameter",
            ));
        }
    };

    eval_expr(&mut env, &lambda.body)
}

fn eval_binary(op: BinaryOp, lhs: Value, rhs: Value, span: SourceSpan) -> Result<Value, EvalError> {
    match op {
        BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div => {
            let (Value::Int(l), Value::Int(r)) = (lhs, rhs) else {
                return Err(EvalError::invariant_due_to_type_check(
                    span,
                    "arithmetic operands did not evaluate to Ints",
                ));
            };

            let (symbol, result) = match op {
                BinaryOp::Add => ("+", l.checked_add(r)),
                BinaryOp::Sub => ("-", l.checked_sub(r)),
                BinaryOp::Mul => ("*", l.checked_mul(r)),
                BinaryOp::Div if r == 0 => return Err(EvalError::DivisionByZero { span }),

                BinaryOp::Div => ("/", l.checked_div(r)),
                _ => unreachable!(),
            };

            result.map(Value::Int).ok_or(EvalError::ArithmeticOverflow {
                operator: symbol,
                lhs: l,
                rhs: r,
                span,
            })
        }

        BinaryOp::Lt
        | BinaryOp::Gt
        | BinaryOp::Le
        | BinaryOp::Ge
        | BinaryOp::Eq
        | BinaryOp::Neq => {
            let (Value::Int(l), Value::Int(r)) = (lhs, rhs) else {
                return Err(EvalError::invariant_due_to_type_check(
                    span,
                    "comparison operands did not evaluate to Ints",
                ));
            };

            Ok(Value::Bool(match op {
                BinaryOp::Lt => l < r,
                BinaryOp::Gt => l > r,
                BinaryOp::Le => l <= r,
                BinaryOp::Ge => l >= r,
                BinaryOp::Eq => l == r,
                BinaryOp::Neq => l != r,
                _ => unreachable!(),
            }))
        }

        BinaryOp::And | BinaryOp::Or => {
            let (Value::Bool(l), Value::Bool(r)) = (lhs, rhs) else {
                return Err(EvalError::invariant_due_to_type_check(
                    span,
                    "logical operands did not evaluate to Bools",
                ));
            };

            Ok(Value::Bool(match op {
                BinaryOp::And => l && r,
                BinaryOp::Or => l || r,
                _ => unreachable!(),
            }))
        }
        BinaryOp::Pipe => {
            let output = apply(rhs, Some(lhs), span)?;
            Ok(output)
        }
    }
}

fn eval_unary(op: UnaryOp, operand: Value, span: SourceSpan) -> Result<Value, EvalError> {
    match op {
        UnaryOp::Neg => {
            let Value::Int(n) = operand else {
                return Err(EvalError::invariant_due_to_type_check(
                    span,
                    "negation operand did not evaluate to an Int",
                ));
            };

            n.checked_neg()
                .map(Value::Int)
                .ok_or(EvalError::ArithmeticOverflow {
                    operator: "-",
                    lhs: 0,
                    rhs: n,
                    span,
                })
        }
        UnaryOp::Not => {
            let Value::Bool(b) = operand else {
                return Err(EvalError::invariant_due_to_type_check(
                    span,
                    "`!` operand did not evaluate to a Bool",
                ));
            };
            Ok(Value::Bool(!b))
        }
    }
}
