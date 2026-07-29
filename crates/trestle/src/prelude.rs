//! The prelude: the type bindings every program starts with.
//!
//! Builtins get no special variant anywhere in the compiler — they are ordinary type bindings that
//! happen to be in scope before the first line of source. [`binding_resolution::resolve`] seeds
//! them into the type namespace *before* any user declaration, so `PRELUDE_TYPES[i]` is
//! `TypeBindingId(i)`. That positional correspondence is how type checking recovers the [`Type`]
//! behind each name: name resolution only ever knows the names.
//!
//! [`binding_resolution::resolve`]: crate::binding_resolution::resolve

use miette::SourceSpan;

use crate::type_check::typed_ast::{Literal, Type};

/// A builtin type: the name it is bound to, and the [`Type`] that name stands for.
pub struct PreludeType {
    pub name: &'static str,
    pub ty: Type,
}

/// The builtin types, in binding order. Const-constructible because none of these variants holds a
/// `Box` — adding a builtin whose `Type` does (an `Fn`, say) would need a `LazyLock` instead.
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
    // `Type::Literal(Literal::Unit)`, not `Type::Unit`: it has to match what the unit literal `()`
    // synthesises, since `unify` treats the two spellings as distinct types.
    PreludeType {
        name: "Unit",
        ty: Type::Literal(Literal::Unit),
    },
];

/// The names only — all name resolution needs. Deliberately narrower than [`PRELUDE_TYPES`]: the
/// binding-resolution pass imports this and so *cannot* reach a `Type` through it, which is the
/// "no type logic in name resolution" split enforced by the import rather than by convention.
pub fn type_names() -> impl Iterator<Item = &'static str> {
    PRELUDE_TYPES.iter().map(|prelude_type| prelude_type.name)
}

/// Prelude bindings have no source text; a zero-length span at offset 0 stands in for "builtin".
pub fn prelude_span() -> SourceSpan {
    SourceSpan::from(0..0)
}
