use std::marker::PhantomData;

use crate::binding_resolution::binding_resolved::TypeBindingId;
use crate::binding_resolution::{BindingId, ResolvedBinding};

use super::error::TypeCheckError;
use super::typed_ast::{Type, TypedBinding};
use super::unification::UnificationMap;

pub(super) trait Indexable {
    fn get_index(self) -> usize;
    fn new(val: usize) -> Self;
}

impl Indexable for BindingId {
    fn get_index(self) -> usize {
        self.0
    }

    fn new(val: usize) -> Self {
        BindingId(val)
    }
}

impl Indexable for TypeBindingId {
    fn get_index(self) -> usize {
        self.0
    }

    fn new(val: usize) -> Self {
        TypeBindingId(val)
    }
}

pub(super) struct GenericTypeMap<TBindingId> {
    types: Vec<Option<Type>>,
    marker: PhantomData<fn() -> TBindingId>,
}

impl<TBindingId: Indexable> GenericTypeMap<TBindingId> {
    pub(super) fn new(binding_count: usize) -> GenericTypeMap<TBindingId> {
        GenericTypeMap {
            types: vec![None; binding_count],
            marker: PhantomData,
        }
    }

    pub(super) fn set(&mut self, id: TBindingId, ty: Type) {
        self.types[Indexable::get_index(id)] = Some(ty);
    }

    pub(super) fn get(&self, id: TBindingId) -> Option<&Type> {
        self.types[Indexable::get_index(id)].as_ref()
    }
}

pub(super) type BindingToTypeMap = GenericTypeMap<BindingId>;
pub(super) type TypeBindingToTypeMap = GenericTypeMap<TypeBindingId>;

pub(super) trait BindingLookup {
    fn lookup(&self, id: BindingId) -> &ResolvedBinding;
}

impl BindingLookup for [ResolvedBinding] {
    fn lookup(&self, id: BindingId) -> &ResolvedBinding {
        &self[id.0]
    }
}

pub(super) fn attach_types_to_bindings<TBindingId: Indexable>(
    bindings: Vec<ResolvedBinding>,
    binding_type_map: &GenericTypeMap<TBindingId>,
    unification_map: &UnificationMap,
) -> Result<Vec<TypedBinding>, TypeCheckError> {
    assert_eq!(bindings.len(), binding_type_map.types.len());

    bindings
        .into_iter()
        .enumerate()
        .map(
            |(index, binding)| match binding_type_map.get(TBindingId::new(index)) {
                Some(ty) => Ok(TypedBinding {
                    name: binding.name,
                    ty: unification_map.subsitute(ty),
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
