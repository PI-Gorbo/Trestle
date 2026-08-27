use super::typed_ast::{ExpressionKind, TypeCheckedExpression};
use super::unification::UnificationMap;

pub(super) fn substitute_in_expr(map: &UnificationMap, expr: &mut TypeCheckedExpression) {
    match &mut expr.kind {
        ExpressionKind::Literal(_) => {}
        ExpressionKind::Var(_) => {}
        ExpressionKind::Binary(_, lhs, rhs) => {
            substitute_in_expr(map, lhs);
            substitute_in_expr(map, rhs);
        }
        ExpressionKind::Unary(_, operand) => {
            substitute_in_expr(map, operand);
        }
        ExpressionKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            substitute_in_expr(map, condition);
            substitute_in_expr(map, then_branch);
            if let Some(else_branch) = else_branch {
                substitute_in_expr(map, else_branch);
            }
        }
        ExpressionKind::Lambda(lambda) => {
            if let Some(param) = &mut lambda.parameter {
                param.ty = map.subsitute(&param.ty);
            }
            substitute_in_expr(map, &mut lambda.body);
        }
        ExpressionKind::FunctionInvocation {
            function,
            arguments,
        } => {
            substitute_in_expr(map, function);

            for arg in arguments {
                substitute_in_expr(map, arg);
            }
        }
        ExpressionKind::Let { value, .. } => {
            substitute_in_expr(map, value);
        }
        ExpressionKind::FieldAccess { target, .. } => {
            substitute_in_expr(map, target);
        }
        ExpressionKind::Block(typed_expressions) => {
            for e in typed_expressions {
                substitute_in_expr(map, e);
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
