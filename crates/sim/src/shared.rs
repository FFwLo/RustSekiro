//! Immutable extracted assets, keyed by type and source path while their owners remain alive.
use std::{
    any::{Any, TypeId},
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex, OnceLock, Weak},
};

type Asset = Arc<dyn Any + Send + Sync>;
type Cell = Arc<Mutex<Option<Weak<dyn Any + Send + Sync>>>>;
type Assets = HashMap<(TypeId, PathBuf), Cell>;

pub(crate) fn load<T: Any + Send + Sync>(
    path: PathBuf,
    read: impl FnOnce() -> Option<T>,
) -> Option<Arc<T>> {
    static ASSETS: OnceLock<Mutex<Assets>> = OnceLock::new();
    let path = path.canonicalize().unwrap_or(path);
    let cell = ASSETS
        .get_or_init(Default::default)
        .lock()
        .unwrap()
        .entry((TypeId::of::<T>(), path))
        .or_default()
        .clone();
    let mut cached = cell.lock().unwrap();
    if let Some(asset) = cached.as_ref().and_then(Weak::upgrade) {
        return asset.downcast().ok();
    }
    let asset = Arc::new(read()?);
    let erased: Asset = asset.clone();
    *cached = Some(Arc::downgrade(&erased));
    Some(asset)
}

/// A combat kernel borrows the encounter's Wolf while stepping; otherwise its slot is empty.
pub struct BehaviorSlot(Option<crate::player::PlayerBehavior>);

impl BehaviorSlot {
    pub fn empty() -> Self {
        Self(None)
    }
    pub fn is_present(&self) -> bool {
        self.0.is_some()
    }
}
impl From<crate::player::PlayerBehavior> for BehaviorSlot {
    fn from(value: crate::player::PlayerBehavior) -> Self {
        Self(Some(value))
    }
}
impl std::ops::Deref for BehaviorSlot {
    type Target = crate::player::PlayerBehavior;
    fn deref(&self) -> &Self::Target {
        self.0
            .as_ref()
            .expect("combat kernel does not currently own Wolf")
    }
}
impl std::ops::DerefMut for BehaviorSlot {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0
            .as_mut()
            .expect("combat kernel does not currently own Wolf")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_shares_live_assets_and_reloads_after_last_owner_drops() {
        let path = PathBuf::from("shared-cache-lifetime-test");
        let first = load(path.clone(), || Some(vec![1u8])).unwrap();
        let second = load(path.clone(), || panic!("must reuse live asset")).unwrap();
        assert!(Arc::ptr_eq(&first, &second));
        let weak = Arc::downgrade(&first);
        drop(first);
        drop(second);
        assert!(weak.upgrade().is_none());
        assert_eq!(*load(path, || Some(vec![2u8])).unwrap(), [2]);
    }
}
