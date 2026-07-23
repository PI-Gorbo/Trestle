//! Final substitution pass: once inference has solved every type variable, walk the typed tree and
//! rewrite each node's `ty` (and each lambda parameter's) to its resolved representative.

use super::typed_ast::{ExpressionKind, TypeCheckedExpression};
use super::unification::UnificationMap;

pub(super) fn subsitute(map: &UnificationMap, expr: &mut TypeCheckedExpression) {
    // The tree shape is unchanged — only the `ty` fields get rewritten — so walk the boxed/vec
    // children by `&mut` (deref coercion turns `&mut Box<_>` into `&mut TypeCheckedExpression`)
    // and reuse every existing allocation.
    match &mut expr.kind {
        ExpressionKind::Literal(_) => {}
        ExpressionKind::Var(_) => {}
        ExpressionKind::Binary(_, lhs, rhs) => {
            subsitute(map, lhs);
            subsitute(map, rhs);
        }
        ExpressionKind::Unary(_, operand) => {
            subsitute(map, operand);
        }
        ExpressionKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            subsitute(map, condition);
            subsitute(map, then_branch);
            if let Some(else_branch) = else_branch {
                subsitute(map, else_branch);
            }
        }
        ExpressionKind::Lambda(lambda) => {
            if let Some(param) = &mut lambda.parameter {
                param.ty = map.subsitute(&param.ty);
            }
            subsitute(map, &mut lambda.body);
        }
        ExpressionKind::FunctionInvocation(_, typed_expressions) => {
            for arg in typed_expressions {
                subsitute(map, arg);
            }
        }
        ExpressionKind::Let { value, .. } => {
            subsitute(map, value);
        }
        ExpressionKind::Block(typed_expressions) => {
            for e in typed_expressions {
                subsitute(map, e);
            }
        }
    }

    expr.ty = map.subsitute(&expr.ty);
}
