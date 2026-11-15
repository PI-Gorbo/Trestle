use std::collections::BTreeMap;

use miette::SourceSpan;

use crate::binding_resolution::BindingId;
use crate::binding_resolution::binding_resolved::TypeBindingId;
use crate::parse::ast::{BinaryOp, UnaryOp};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TypeVarId(pub usize);

#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    Unit,
    Literal(Literal),
    Var(TypeVarId),
    Fn {
        parameter: Option<Box<Type>>,
        return_type: Box<Type>,
    },
    Record(BTreeMap<String, Box<Type>>),
}

impl Type {
    pub(super) fn fold_free_type_variables<TAgg>(
        &self,
        folder: &mut impl FnMut(&TypeVarId, TAgg) -> TAgg,
        initial_value: TAgg,
    ) -> TAgg {
        match self {
            Type::Var(v) => folder(v, initial_value),
            Type::Unit | Type::Literal(_) => initial_value,
            Type::Fn {
                parameter: param,
                return_type: result,
            } => {
                let agg1 = match param {
                    None => initial_value,
                    Some(param) => param.fold_free_type_variables(folder, initial_value),
                };

                result.fold_free_type_variables(folder, agg1)
            }
            Type::Record(btree_map) => btree_map.iter().fold(initial_value, |agg, (_, field)| {
                field.as_ref().fold_free_type_variables(folder, agg)
            }),
        }
    }

    pub(super) fn map_free_type_variables(
        &self,
        mapper: &mut impl FnMut(&TypeVarId) -> Type,
    ) -> Self {
        match self {
            Type::Var(v) => mapper(v),
            Type::Unit | Type::Literal(_) => self.clone(),
            Type::Fn {
                parameter: param,
                return_type: result,
            } => Type::Fn {
                parameter: param
                    .as_ref()
                    .map(|param| Box::new(param.map_free_type_variables(mapper))),
                return_type: Box::new(result.map_free_type_variables(mapper)),
            },
            Type::Record(btree_map) => Type::Record(
                btree_map
                    .iter()
                    .map(|(name, field)| {
                        (
                            name.clone(),
                            Box::new(field.map_free_type_variables(mapper)),
                        )
                    })
                    .collect(),
            ),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    Int,
    Bool,
    Float,
    String,
    Unit,
}

#[derive(Debug, PartialEq)]
pub struct TypedBinding {
    pub name: String,
    pub ty: Type,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TypeCheckedExpression {
    pub kind: ExpressionKind,
    pub span: SourceSpan,
    pub ty: Type,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TypeCheckedLiteral {
    Int(i64),
    Bool(bool),
    Float(f64),
    String(String),
    Record(BTreeMap<String, TypeCheckedExpression>),
    Unit,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExpressionKind {
    Literal(TypeCheckedLiteral),
    Var(BindingId),
    Binary(
        BinaryOp,
        Box<TypeCheckedExpression>,
        Box<TypeCheckedExpression>,
    ),
    Unary(UnaryOp, Box<TypeCheckedExpression>),
    FunctionInvocation {
        function: Box<TypeCheckedExpression>,
        arguments: Vec<TypeCheckedExpression>,
    },
    If {
        condition: Box<TypeCheckedExpression>,
        then_branch: Box<TypeCheckedExpression>,
        else_branch: Option<Box<TypeCheckedExpression>>,
    },
    Lambda(Lambda),
    Let {
        binding: BindingId,
        value: Box<TypeCheckedExpression>,
    },
    Block(Vec<TypeCheckedExpression>),
    FieldAccess {
        target: Box<TypeCheckedExpression>,
        field_name: String,
    },
    TypeDeclaration {
        identifier: TypeBindingId,
        type_expression: Type,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub binding: BindingId,
    pub ty: Type,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Lambda {
    pub parameter: Option<Param>,
    pub body: Box<TypeCheckedExpression>,
}

#[derive(Debug, PartialEq)]
pub struct TypeCheckedProgram {
    pub expressions: Vec<TypeCheckedExpression>,
    pub bindings: Vec<TypedBinding>,
    pub type_bindings: Vec<TypedBinding>,
}
