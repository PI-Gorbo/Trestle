use std::collections::BTreeMap;

use miette::SourceSpan;

use crate::binding_resolution::BindingId;
use crate::binding_resolution::binding_resolved::TypeBindingId;
use crate::parse::ast::{BinaryOp, UnaryOp};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeVarId(pub usize);

#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    Unit,
    Literal(Literal),
    Var(TypeVarId),
    Fn(Option<Box<Type>>, Box<Type>),
    Record(BTreeMap<String, Box<Type>>),
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
