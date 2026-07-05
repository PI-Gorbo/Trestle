//! Evaluation: tree-walk a [`CheckedProgram`] to a [`Value`].
//!
//! Because name resolution and type checking happen in
//! [`analyse`](crate::analysis::analyse), the errors this stage used to risk (unbound
//! name, arithmetic on a non-`Int`) can't occur for a well-typed program — so
//! [`EvalError`] is empty for now, kept only for later tiers (overflow, effects).

use std::rc::Rc;

use crate::checked::{self, BindingId, CheckedProgram};

/// A runtime value. Replaces the empty `Output` struct.
#[derive(Debug, Clone)]
pub enum Value<'a> {
    Int(i64),
    /// One-param closure (currying is desugared). Borrows the lambda from the checked
    /// tree; captures the environment it was defined in.
    Closure {
        lambda: &'a checked::Lambda,
        env: Environment<'a>,
    },
}

/// Rc-linked cons-chain of scopes, keyed by [`BindingId`]. Cheap to capture in a closure.
#[derive(Debug, Clone, Default)]
pub struct Environment<'a>(Option<Rc<Scope<'a>>>);

#[derive(Debug)]
struct Scope<'a> {
    id: BindingId,
    value: Value<'a>,
    parent: Option<Rc<Scope<'a>>>,
}

impl<'a> Environment<'a> {
    pub fn empty() -> Self {
        Self(None)
    }

    /// New environment with `id -> value` pushed on front (immutable / shared).
    pub fn extend(&self, id: BindingId, value: Value<'a>) -> Self {
        let _ = (id, value);
        todo!()
    }

    pub fn lookup(&self, id: BindingId) -> Option<&Value<'a>> {
        let _ = id;
        todo!()
    }
}

/// Runtime failures. Empty now — resolution/type errors are caught in `analyse`, so a
/// well-typed program can't fault here yet. Kept for later tiers (overflow, effects).
#[derive(Debug)]
pub enum EvalError {}

/// Evaluate a checked program: thread top-level `let`s through the environment and
/// return the value of the last expression.
pub fn evaluate<'a>(
    env: &Environment<'a>,
    program: &'a CheckedProgram,
) -> Result<Value<'a>, EvalError> {
    let _ = (env, program);
    todo!()
}

// Suggested private helper for the recursive walk (add when you implement):
// fn eval_expr<'a>(env: &Environment<'a>, expr: &'a checked::Expression) -> Result<Value<'a>, EvalError>
