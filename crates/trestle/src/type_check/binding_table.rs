//! The per-binding type table built up during inference, and the step that freezes it into the
//! final [`TypeCheckedBinding`] list.

use std::marker::PhantomData;

use crate::binding_resolution::binding_resolved::TypeBindingId;
use crate::binding_resolution::{BindingId, ResolvedBinding};

use super::error::TypeCheckError;
use super::typed_ast::{Type, TypedBinding};
use super::unification::UnificationMap;

/// The id type that indexes a [`GenericTypeMap`] — one impl per namespace, so a map built for
/// values can't be read with a type-binding id (or vice versa).
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

/// Pair each binding with the type computed for it during the walk (**moving** its name across),
/// resolving every solved type variable through `unification_map` on the way — binding types are
/// recorded during inference with their variables intact, so they need the same substitution the
/// expression tree gets. A binding still untyped afterwards is an [`UntypedBindingAfterTypeCheck`]
/// error. Consumes the binding list since it's the last reader of it.
///
/// Generic over the id type so both namespaces — [`BindingId`] values and [`TypeBindingId`] `type`
/// declarations — share one implementation; the map's own parameter picks the right one at each
/// call site.
///
/// [`UntypedBindingAfterTypeCheck`]: TypeCheckError::UntypedBindingAfterTypeCheck
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
