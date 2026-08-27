use miette::SourceSpan;

use crate::type_check::typed_ast::{Literal, Type};

pub struct PreludeType {
    pub name: &'static str,
    pub ty: Type,
}

pub const PRELUDE_TYPES: &[PreludeType] = &[
    PreludeType {
        name: "Int",
        ty: Type::Literal(Literal::Int),
    },
    PreludeType {
        name: "Float",
        ty: Type::Literal(Literal::Float),
    },
    PreludeType {
        name: "String",
        ty: Type::Literal(Literal::String),
    },
    PreludeType {
        name: "Bool",
        ty: Type::Literal(Literal::Bool),
    },
    PreludeType {
        name: "Unit",
        ty: Type::Literal(Literal::Unit),
    },
];

pub fn type_names() -> impl Iterator<Item = &'static str> {
    PRELUDE_TYPES.iter().map(|prelude_type| prelude_type.name)
}

pub fn prelude_span() -> SourceSpan {
    SourceSpan::from(0..0)
}
