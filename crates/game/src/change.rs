//! Change-aware writes shared by component and resource publishers.

use std::collections::BTreeMap;

use bevy::prelude::*;

/// Writes a field, marking its component or resource changed only when the value differs.
pub(crate) fn set<T, V: PartialEq>(
    item: &mut Mut<T>,
    field: impl FnOnce(&mut T) -> &mut V,
    value: V,
) {
    item.reborrow().map_unchanged(field).set_if_neq(value);
}

pub(crate) fn insert_if_neq<K: Ord, V: PartialEq>(mut map: Mut<BTreeMap<K, V>>, key: K, value: V) {
    if map.get(&key) != Some(&value) {
        map.insert(key, value);
    }
}
