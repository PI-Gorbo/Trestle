use std::rc::Rc;

use crate::binding_resolution::binding_resolved::TypeBindingId;

use super::binding_resolved::BindingId;

// Linked list of Rc backed Scope Entries
pub(super) struct GenericScopeNode<T> {
    parent: GenericScope<T>,
    value: T,
}

pub(super) enum GenericScope<T> {
    Empty,
    Cons(Rc<GenericScopeNode<T>>),
}

impl<T> Clone for GenericScope<T> {
    fn clone(&self) -> Self {
        match self {
            Self::Empty => Self::Empty,
            Self::Cons(arg0) => Self::Cons(arg0.clone()),
        }
    }
}

impl<T> GenericScope<T> {
    pub(super) fn extend(&self, value: T) -> GenericScope<T> {
        // O(1): clones one Rc for the parent tail.
        GenericScope::Cons(Rc::new(GenericScopeNode {
            parent: self.clone(),
            value: value,
        }))
    }

    // This provides natural shadowing, where we iterate by taking the current item,
    // then mapping to the next via its parent.
    pub(super) fn iter(&self) -> impl Iterator<Item = &T> {
        std::iter::successors(Some(self), |scope| match scope {
            GenericScope::Cons(node) => Some(&node.parent),
            GenericScope::Empty => None,
        })
        .filter_map(|scope| match scope {
            GenericScope::Cons(node) => Some(&node.value),
            GenericScope::Empty => None,
        })
    }
}

// For variable binding.
pub(super) struct VariableBindingScopeEntry {
    pub(super) name: String,
    pub(super) binding: BindingId,
}

pub(super) type VariableBindingScope = GenericScope<VariableBindingScopeEntry>;

impl VariableBindingScope {
    pub(super) fn lookup(&self, name: &str) -> Option<BindingId> {
        self.iter()
            .find(|entry| entry.name == name)
            .map(|entry| entry.binding)
    }
}

// For type binding
pub(super) struct TypeBindingScopeEntry {
    pub(super) name: String,
    pub(super) binding: TypeBindingId,
}

pub(super) type TypeBindingScope = GenericScope<TypeBindingScopeEntry>;

impl TypeBindingScope {
    pub(super) fn lookup(&self, name: &str) -> Option<TypeBindingId> {
        self.iter()
            .find(|entry| entry.name == name)
            .map(|entry| entry.binding)
    }
}

#[derive(Clone)]
pub(super) struct Scope {
    type_scope: TypeBindingScope,
    variable_scope: VariableBindingScope,
}

impl Scope {
    pub(super) fn new() -> Scope {
        Scope {
            type_scope: TypeBindingScope::Empty,
            variable_scope: VariableBindingScope::Empty,
        }
    }

    /// A new scope with `entry` bound in the variable namespace; the type namespace is carried
    /// through untouched. O(1) — the untouched namespace costs one `Rc` clone, not a copy.
    pub(super) fn extend_variable(&self, entry: VariableBindingScopeEntry) -> Scope {
        Scope {
            type_scope: self.type_scope.clone(),
            variable_scope: self.variable_scope.extend(entry),
        }
    }

    /// The type-namespace twin of [`Scope::extend_variable`].
    #[allow(dead_code, reason = "no caller until the `TypeDeclaration` arm is resolved")]
    pub(super) fn extend_type(&self, entry: TypeBindingScopeEntry) -> Scope {
        Scope {
            type_scope: self.type_scope.extend(entry),
            variable_scope: self.variable_scope.clone(),
        }
    }

    /// Unqualified `lookup` is the *variable* namespace — the one every expression-position name
    /// (`Var`, a call's callee) resolves against.
    pub(super) fn lookup(&self, name: &str) -> Option<BindingId> {
        self.variable_scope.lookup(name)
    }

    #[allow(dead_code, reason = "no caller until the `TypeDeclaration` arm is resolved")]
    pub(super) fn lookup_type(&self, name: &str) -> Option<TypeBindingId> {
        self.type_scope.lookup(name)
    }
}
