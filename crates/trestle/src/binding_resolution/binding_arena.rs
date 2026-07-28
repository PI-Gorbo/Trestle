use super::binding_resolved::{BindingId, ResolvedBinding};

/// The binding table under construction. A [`BindingId`] is exactly an index into this
/// arena, so id-minting lives here to keep that invariant local.
pub(super) struct BindingArena(Vec<ResolvedBinding>);

impl BindingArena {
    pub(super) fn new() -> BindingArena {
        BindingArena(Vec::new())
    }

    /// The id the *next* pushed binding will receive.
    pub(super) fn mint_binding_id(&self) -> BindingId {
        BindingId(self.0.len())
    }

    pub(super) fn push(&mut self, binding: ResolvedBinding) {
        self.0.push(binding);
    }

    /// Consume into the plain vec `BindingResolvedProgram` expects.
    pub(super) fn into_bindings(self) -> Vec<ResolvedBinding> {
        self.0
    }
}
