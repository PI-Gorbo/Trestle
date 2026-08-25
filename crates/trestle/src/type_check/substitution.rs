//! Final substitution pass: once inference has solved every type variable, walk the typed tree and
//! rewrite each node's `ty` (and each lambda parameter's) to its resolved representative.

use super::typed_ast::{ExpressionKind, TypeCheckedExpression};
use super::unification::UnificationMap;

pub(super) fn subsitute_in_expr(map: &UnificationMap, expr: &mut TypeCheckedExpression) {
    // The tree shape is unchanged — only the `ty` fields get rewritten — so walk the boxed/vec
    // children by `&mut` (deref coercion turns `&mut Box<_>` into `&mut TypeCheckedExpression`)
    // and reuse every existing allocation.
    match &mut expr.kind {
        ExpressionKind::Literal(_) => {}
        ExpressionKind::Var(_) => {}
        ExpressionKind::Binary(_, lhs, rhs) => {
            subsitute_in_expr(map, lhs);
            subsitute_in_expr(map, rhs);
        }
        ExpressionKind::Unary(_, operand) => {
            subsitute_in_expr(map, operand);
        }
        ExpressionKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            subsitute_in_expr(map, condition);
            subsitute_in_expr(map, then_branch);
            if let Some(else_branch) = else_branch {
                subsitute_in_expr(map, else_branch);
            }
        }
        ExpressionKind::Lambda(lambda) => {
            if let Some(param) = &mut lambda.parameter {
                param.ty = map.subsitute(&param.ty);
            }
            subsitute_in_expr(map, &mut lambda.body);
        }
        ExpressionKind::FunctionInvocation {
            function,
            arguments,
        } => {
            subsitute_in_expr(map, function);

            for arg in arguments {
                subsitute_in_expr(map, arg);
            }
        }
        ExpressionKind::Let { value, .. } => {
            subsitute_in_expr(map, value);
        }
        ExpressionKind::FieldAccess { target, .. } => {
            subsitute_in_expr(map, target);
        }
        ExpressionKind::Block(typed_expressions) => {
            for e in typed_expressions {
                subsitute_in_expr(map, e);
            }
        }
        ExpressionKind::TypeDeclaration {
            identifier: _,
            type_expression,
        } => {
            *type_expression = map.subsitute(type_expression);
        }
    }

    expr.ty = map.subsitute(&expr.ty);
}
