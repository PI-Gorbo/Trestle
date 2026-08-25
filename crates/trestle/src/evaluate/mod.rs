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

/// A runtime value. Replaces the empty `Output` struct.
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

/// Rc-linked cons-chain of scopes, keyed by [`BindingId`]. Cheap to capture in a closure.
/// Principles
///  SHARE     clone = bump a counter + copy a pointer     (never copies Scope)
///  PERSIST   extend = new front node, old tail reused   (never mutates old nodes)
///  RECLAIM   drop = decrement; free only at zero         (last owner cleans up)
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

    /// New environment with `id -> value` pushed on front (immutable / shared).
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

/// Evaluate a checked program: thread top-level `let`s through the environment and
/// return the value of the last expression (`Unit` for an empty program).
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

/// `env` is in/out, mirroring the outgoing scope
/// [`resolve_expression`](crate::binding_resolution) hands back: only a declaration arm
/// (`Let`) writes to it, so the other arms recurse through the same `&mut` for free.
///
/// A `let` is an ordinary expression in the grammar, so it can appear anywhere — an `if`
/// branch, a call argument — not only as a sequence element. A binding it makes in one of
/// those positions rides out into `env`, which is harmless: the environment is keyed by
/// [`BindingId`], and binding resolution discards a sub-expression's outgoing *scope*, so no
/// later expression can name that id. The escapee is invisible to `lookup`, not merely
/// unused. Only `Block` (and a lambda body, via `apply`) opens a real scope, and only those
/// pay for a child environment.
fn eval_expr(env: &mut Environment, expr: &TypeCheckedExpression) -> Result<Value, EvalError> {
    match &expr.kind {
        ExpressionKind::Literal(literal) => Ok(match literal {
            TypeCheckedLiteral::Int(value) => Value::Int(*value),
            TypeCheckedLiteral::Bool(value) => Value::Bool(*value),
            TypeCheckedLiteral::Float(value) => Value::Float(*value),
            // The string is stored verbatim (quotes included) — carry it through as-is.
            TypeCheckedLiteral::String(value) => Value::String(value.clone()),
            TypeCheckedLiteral::Unit => Value::Unit,
            TypeCheckedLiteral::Record(fields) => Value::Record(
                fields
                    .iter()
                    .map(|(name, field)| Ok((name.clone(), eval_expr(env, field)?)))
                    .collect::<Result<BTreeMap<_, _>, EvalError>>()?,
            ),
        }),

        // Name resolution guarantees the binding exists by the time we reach its use.
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

        // A `let` binds into the surrounding sequence rather than nesting a body: eval its
        // value, extend the environment for whatever follows, and evaluate to `Unit`.
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
                // No else and a false condition: there is no value to produce yet.
                (false, None) => Ok(Value::Unit),
            }
        }

        // A lambda captures the environment it closed over; currying is already desugared,
        // so this is always a one-parameter (or nullary) closure.
        ExpressionKind::Lambda(lambda) => Ok(Value::Closure {
            lambda: Rc::new(lambda.clone()),
            env: env.clone(),
        }),

        // Fold the arguments through the callee one at a time (currying). Applying fewer
        // arguments than the function takes leaves an intermediate closure (partial
        // application).
        ExpressionKind::FunctionInvocation {
            function,
            arguments,
        } => {
            let evaluated_funciton = eval_expr(env, function)?;
            // A zero-arg call `f()` invokes a nullary closure once — there are no arguments to
            // fold, but the call must still run the body (`apply` discards the unit argument).
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

        // The one arm that opens a scope of its own: a block's `let`s are threaded through a
        // child environment, dropped at the closing brace so they don't outlive it. The clone
        // is one `Rc` bump (see `Environment`), not a copy of the chain.
        ExpressionKind::Block(exprs) => {
            let mut inner = env.clone();
            let mut result = Value::Unit;
            for expr in exprs {
                result = eval_expr(&mut inner, expr)?;
            }

            Ok(result)
        }

        // Type checking guarantees the target is a record and that the field exists on it, so
        // both misses below are upstream bugs rather than user errors.
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

/// Apply one argument to a closure: bind the parameter in the closure's captured
/// environment and evaluate its body.
///
/// `span` points at the callee, and is only read to report a broken type-checker guarantee —
/// `Value` carries no span of its own, so it has to come down from the call site.
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

/// `span` covers the whole operation — see [`apply`] for why it is threaded in.
fn eval_binary(op: BinaryOp, lhs: Value, rhs: Value, span: SourceSpan) -> Result<Value, EvalError> {
    match op {
        // Arithmetic: Int × Int → Int.
        BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div => {
            let (Value::Int(l), Value::Int(r)) = (lhs, rhs) else {
                return Err(EvalError::invariant_due_to_type_check(
                    span,
                    "arithmetic operands did not evaluate to Ints",
                ));
            };

            // Checked, not bare: `l / r` traps on a zero divisor at every optimisation level,
            // and `+ - *` panic on overflow in debug while wrapping in release — so the build
            // under test and the build we ship would disagree. Both faults are reachable from
            // a well-typed program, which makes them `EvalError`s rather than panics.
            let (symbol, result) = match op {
                BinaryOp::Add => ("+", l.checked_add(r)),
                BinaryOp::Sub => ("-", l.checked_sub(r)),
                BinaryOp::Mul => ("*", l.checked_mul(r)),
                BinaryOp::Div if r == 0 => return Err(EvalError::DivisionByZero { span }),
                // `checked_div` still returns `None` for `i64::MIN / -1`, whose true value is
                // one past `i64::MAX`. With the zero case already handled above, that is the
                // only way this arm yields `None`, and overflow is the honest name for it.
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
        // Comparison: Int × Int → Bool.
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
        // Logical combinators: Bool × Bool → Bool.
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

/// `span` covers the operand — see [`apply`] for why it is threaded in.
fn eval_unary(op: UnaryOp, operand: Value, span: SourceSpan) -> Result<Value, EvalError> {
    match op {
        UnaryOp::Neg => {
            let Value::Int(n) = operand else {
                return Err(EvalError::invariant_due_to_type_check(
                    span,
                    "negation operand did not evaluate to an Int",
                ));
            };
            // `-i64::MIN` overflows. Not reachable from a literal — the parser rejects
            // anything past `i64::MAX` — but arithmetic can land exactly on `i64::MIN`.
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
