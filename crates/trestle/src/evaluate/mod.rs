//! Evaluation: tree-walk an [`AnalysedProgram`] to a [`Value`].
//!
//! Because name resolution and type checking happen in
//! [`analyse`](crate::analyse::analyse), the errors this stage used to risk (unbound
//! name, arithmetic on a non-`Int`) can't occur for a well-typed program — so
//! [`EvalError`] is empty for now, kept only for later tiers (overflow, effects).

use std::rc::Rc;

use crate::analyse::analysed::{
    self, AnalysedExpression, AnalysedLiteral, AnalysedProgram, BindingId, ExpressionKind,
};
use crate::parse::ast::{BinaryOp, UnaryOp};

/// A runtime value. Replaces the empty `Output` struct.
#[derive(Debug, Clone)]
pub enum Value {
    Int(i64),
    Bool(bool),
    Float(f64),
    String(String),
    /// The value of a `let` (and of a program/block that ends in one).
    Unit,
    /// One-param closure (currying is desugared). Owns the lambda (via `Rc`, so cloning a
    /// `Value` stays cheap) and captures the environment it was defined in.
    Closure {
        lambda: Rc<analysed::Lambda>,
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

/// Runtime failures. Empty now — resolution/type errors are caught in `analyse`, so a
/// well-typed program can't fault here yet. Kept for later tiers (overflow, effects).
#[derive(Debug)]
pub enum EvalError {}

/// Evaluate a checked program: thread top-level `let`s through the environment and
/// return the value of the last expression.
pub fn evaluate(program: AnalysedProgram) -> Result<Value, EvalError> {
    eval_block(&Environment::empty(), &program.expressions)
}

/// Run a sequence of expressions, threading each `let` binding into the environment for
/// the ones that follow, and yield the value of the last expression (`Unit` if empty).
///
/// Shared by the top-level program and the `Block` expression — a block is just a nested
/// sub-program with its own scope.
fn eval_block(env: &Environment, exprs: &[AnalysedExpression]) -> Result<Value, EvalError> {
    let mut env = env.clone();
    let mut result = Value::Unit;
    for expr in exprs {
        match &expr.kind {
            // A `let` binds into the surrounding sequence rather than nesting a body: eval
            // its value, extend the scope for later siblings, and evaluate to `Unit`.
            ExpressionKind::Let { binding, value } => {
                let bound = eval_expr(&env, value)?;
                env = env.extend(*binding, bound);
                result = Value::Unit;
            }
            _ => result = eval_expr(&env, expr)?,
        }
    }
    Ok(result)
}

fn eval_expr(env: &Environment, expr: &AnalysedExpression) -> Result<Value, EvalError> {
    match &expr.kind {
        ExpressionKind::Literal(literal) => Ok(match literal {
            AnalysedLiteral::Int(value) => Value::Int(*value as i64),
            AnalysedLiteral::Bool(value) => Value::Bool(*value),
            AnalysedLiteral::Float(value) => Value::Float(*value),
            // The string is stored verbatim (quotes included) — carry it through as-is.
            AnalysedLiteral::String(value) => Value::String(value.clone()),
        }),

        // Name resolution guarantees the binding exists by the time we reach its use.
        ExpressionKind::Var(id) => Ok(env
            .lookup(*id)
            .expect("resolved variable is bound in the environment")
            .clone()),

        ExpressionKind::Binary(op, lhs, rhs) => {
            let lhs = eval_expr(env, lhs)?;
            let rhs = eval_expr(env, rhs)?;
            Ok(eval_binary(*op, lhs, rhs))
        }

        ExpressionKind::Unary(op, operand) => {
            let operand = eval_expr(env, operand)?;
            Ok(eval_unary(*op, operand))
        }

        ExpressionKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            let Value::Bool(taken) = eval_expr(env, condition)? else {
                unreachable!("if condition type-checks as Bool");
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
        ExpressionKind::FunctionInvocation(callee, args) => {
            let mut callee = env
                .lookup(*callee)
                .expect("resolved function is bound in the environment")
                .clone();
            for arg in args {
                let arg = eval_expr(env, arg)?;
                callee = apply(callee, arg)?;
            }
            Ok(callee)
        }

        ExpressionKind::Block(exprs) => eval_block(env, exprs),

        // `let` only appears as a sequence element; `eval_block` intercepts it there.
        ExpressionKind::Let { .. } => {
            unreachable!("let is threaded by eval_block, never reached as a sub-expression")
        }
    }
}

/// Apply one argument to a closure: bind the parameter in the closure's captured
/// environment and evaluate its body.
fn apply(callee: Value, arg: Value) -> Result<Value, EvalError> {
    let Value::Closure { lambda, env } = callee else {
        unreachable!("callee type-checks as a function");
    };
    let env = match &lambda.parameter {
        Some(param) => env.extend(param.binding, arg),
        None => env,
    };
    eval_expr(&env, &lambda.body)
}

fn eval_binary(op: BinaryOp, lhs: Value, rhs: Value) -> Value {
    match op {
        // Arithmetic: Int × Int → Int.
        BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div => {
            let (Value::Int(l), Value::Int(r)) = (lhs, rhs) else {
                unreachable!("arithmetic operands type-check as Int");
            };
            Value::Int(match op {
                BinaryOp::Add => l + r,
                BinaryOp::Sub => l - r,
                BinaryOp::Mul => l * r,
                BinaryOp::Div => l / r,
                _ => unreachable!(),
            })
        }
        // Comparison: Int × Int → Bool.
        BinaryOp::Lt | BinaryOp::Gt | BinaryOp::Le | BinaryOp::Ge | BinaryOp::Eq | BinaryOp::Neq => {
            let (Value::Int(l), Value::Int(r)) = (lhs, rhs) else {
                unreachable!("comparison operands type-check as Int");
            };
            Value::Bool(match op {
                BinaryOp::Lt => l < r,
                BinaryOp::Gt => l > r,
                BinaryOp::Le => l <= r,
                BinaryOp::Ge => l >= r,
                BinaryOp::Eq => l == r,
                BinaryOp::Neq => l != r,
                _ => unreachable!(),
            })
        }
        // Logical combinators: Bool × Bool → Bool.
        BinaryOp::And | BinaryOp::Or => {
            let (Value::Bool(l), Value::Bool(r)) = (lhs, rhs) else {
                unreachable!("logical operands type-check as Bool");
            };
            Value::Bool(match op {
                BinaryOp::And => l && r,
                BinaryOp::Or => l || r,
                _ => unreachable!(),
            })
        }
    }
}

fn eval_unary(op: UnaryOp, operand: Value) -> Value {
    match op {
        UnaryOp::Neg => {
            let Value::Int(n) = operand else {
                unreachable!("negation operand type-checks as Int");
            };
            Value::Int(-n)
        }
        UnaryOp::Not => {
            let Value::Bool(b) = operand else {
                unreachable!("`!` operand type-checks as Bool");
            };
            Value::Bool(!b)
        }
    }
}
