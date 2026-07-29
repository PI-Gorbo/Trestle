use std::marker::PhantomData;

use crate::binding_resolution::binding_resolved::TypeBindingId;

use super::binding_resolved::{BindingId, ResolvedBinding};

pub(super) trait ArenaId {
    fn from_index(index: usize) -> Self;
}

impl ArenaId for BindingId {
    fn from_index(index: usize) -> Self {
        BindingId(index)
    }
}

impl ArenaId for TypeBindingId {
    fn from_index(index: usize) -> Self {
        TypeBindingId(index)
    }
}

pub(super) struct GenericArena<TItem, TArenaId> {
    entries: Vec<TItem>,

    // INTERESTING???
    marker: PhantomData<fn() -> TArenaId>,
}

impl<TItem, TAreanaId: ArenaId> GenericArena<TItem, TAreanaId> {
    pub(super) fn new() -> GenericArena<TItem, TAreanaId> {
        GenericArena {
            entries: Vec::new(),
            marker: PhantomData,
        }
    }

    pub(super) fn extend(&mut self, item: TItem) -> TAreanaId {
        let new_id = ArenaId::from_index(self.entries.len());

        self.entries.push(item);

        new_id
    }

    pub(super) fn into_vec(self) -> Vec<TItem> {
        self.entries
    }
}

pub(super) type VariableBindingArena = GenericArena<ResolvedBinding, BindingId>;
pub(super) type TypeBindingArea = GenericArena<ResolvedBinding, TypeBindingId>;
