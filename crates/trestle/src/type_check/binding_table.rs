//! The per-binding type table built up during inference, and the step that freezes it into the
//! final [`TypeCheckedBinding`] list.

use crate::binding_resolution::{BindingId, ResolvedBinding};

use super::error::TypeCheckError;
use super::typed_ast::{Type, TypeCheckedBinding};

pub(super) struct BindingToTypeMap {
    types: Vec<Option<Type>>,
}

impl BindingToTypeMap {
    pub(super) fn new(binding_count: usize) -> BindingToTypeMap {
        BindingToTypeMap {
            types: vec![None; binding_count],
        }
    }

    pub(super) fn set(&mut self, id: BindingId, ty: Type) {
        self.types[id.0] = Some(ty);
    }

    pub(super) fn get(&self, id: BindingId) -> Option<&Type> {
        self.types[id.0].as_ref()
    }
}

pub(super) trait BindingLookup {
    fn lookup(&self, id: BindingId) -> &ResolvedBinding;
}

impl BindingLookup for [ResolvedBinding] {
    fn lookup(&self, id: BindingId) -> &ResolvedBinding {
        &self[id.0]
    }
}

/// Pair each binding with the type computed for it during the walk, **moving** its name across.
/// A binding still untyped afterwards is an [`UntypedBindingAfterTypeCheck`] error. Consumes the
/// binding table since it's the last reader of it.
///
/// [`UntypedBindingAfterTypeCheck`]: TypeCheckError::UntypedBindingAfterTypeCheck
pub(super) fn zip_bindings_with_types(
    bindings: Vec<ResolvedBinding>,
    binding_type_map: &BindingToTypeMap,
) -> Result<Vec<TypeCheckedBinding>, TypeCheckError> {
    assert_eq!(bindings.len(), binding_type_map.types.len());

    bindings
        .into_iter()
        .enumerate()
        .map(
            |(index, binding)| match binding_type_map.get(BindingId(index)) {
                Some(ty) => Ok(TypeCheckedBinding {
                    name: binding.name,
                    ty: ty.clone(),
                    span: binding.span,
                }),
                None => Err(TypeCheckError::UntypedBindingAfterTypeCheck {
                    name: binding.name,
                    span: binding.span,
                }),
            },
        )
        .collect()
}
