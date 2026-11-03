//! Lua tables with an array part and an insertion-ordered hash part.
//!
//! Iteration order of the hash part is insertion order, which keeps `pairs` deterministic.
//! The real HKS runtime iterates in hash order; scripts that depend on that order are rare and
//! documented in `docs/HKS-COMBAT.md`.

use crate::value::{Key, Value};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

pub type TableRef = Rc<RefCell<Table>>;

#[derive(Default)]
pub struct Table {
    /// Values for keys `1..=array.len()`.
    array: Vec<Value>,
    /// Hash entries in insertion order; removed entries keep their slot with a nil value.
    entries: Vec<(Key, Value)>,
    index: HashMap<Key, usize>,
    pub metatable: Option<TableRef>,
}

impl Table {
    pub fn new_ref() -> TableRef {
        Rc::new(RefCell::new(Table::default()))
    }

    fn array_index(key: &Value) -> Option<usize> {
        match key {
            Value::Number(n) if n.fract() == 0.0 && *n >= 1.0 && *n <= u32::MAX as f32 => {
                Some(*n as usize)
            }
            _ => None,
        }
    }

    pub fn get(&self, key: &Value) -> Value {
        if let Some(i) = Self::array_index(key)
            && i <= self.array.len()
        {
            return self.array[i - 1].clone();
        }
        match Key::new(key.clone()) {
            Some(k) => self
                .index
                .get(&k)
                .map(|&slot| self.entries[slot].1.clone())
                .unwrap_or_default(),
            None => Value::Nil,
        }
    }

    pub fn get_str(&self, key: &str) -> Value {
        self.get(&Value::str(key))
    }

    pub fn get_int(&self, i: usize) -> Value {
        self.get(&Value::Number(i as f32))
    }

    /// Raw assignment. Errors on a nil or NaN key, like Lua.
    pub fn set(&mut self, key: Value, value: Value) -> Result<(), &'static str> {
        if let Some(i) = Self::array_index(&key) {
            if i <= self.array.len() {
                self.array[i - 1] = value;
                if i == self.array.len() {
                    while matches!(self.array.last(), Some(Value::Nil)) {
                        self.array.pop();
                    }
                }
                return Ok(());
            }
            if i == self.array.len() + 1 && !value.is_nil() {
                self.array.push(value);
                self.remove_hash(&key);
                self.migrate_from_hash();
                return Ok(());
            }
        }
        let Some(k) = Key::new(key) else {
            return Err("table index is nil or NaN");
        };
        match self.index.get(&k) {
            Some(&slot) => self.entries[slot].1 = value,
            None if value.is_nil() => {}
            None => {
                self.index.insert(k.clone(), self.entries.len());
                self.entries.push((k, value));
            }
        }
        Ok(())
    }

    fn remove_hash(&mut self, key: &Value) {
        if let Some(k) = Key::new(key.clone())
            && let Some(&slot) = self.index.get(&k)
        {
            self.entries[slot].1 = Value::Nil;
        }
    }

    /// Moves `len+1, len+2, ...` from the hash part into the array part after an append.
    fn migrate_from_hash(&mut self) {
        loop {
            let next = Value::Number((self.array.len() + 1) as f32);
            let Some(k) = Key::new(next) else { return };
            let Some(&slot) = self.index.get(&k) else {
                return;
            };
            let v = std::mem::take(&mut self.entries[slot].1);
            if v.is_nil() {
                return;
            }
            self.array.push(v);
        }
    }

    /// The length operator: a border of the sequence.
    pub fn len(&self) -> usize {
        let mut n = self.array.len();
        while !self.get_int(n + 1).is_nil() {
            n += 1;
        }
        n
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Lua `next`: the entry after `key` (or the first for nil), skipping nil values.
    pub fn next(&self, key: &Value) -> Result<Option<(Value, Value)>, &'static str> {
        // Position 0..array.len() is the array part, then hash slots.
        let start = if key.is_nil() {
            0
        } else if let Some(i) = Self::array_index(key).filter(|&i| i <= self.array.len()) {
            i
        } else {
            let k = Key::new(key.clone()).ok_or("invalid key to 'next'")?;
            let slot = *self.index.get(&k).ok_or("invalid key to 'next'")?;
            self.array.len() + slot + 1
        };
        for pos in start..self.array.len() + self.entries.len() {
            if pos < self.array.len() {
                if !self.array[pos].is_nil() {
                    return Ok(Some((
                        Value::Number((pos + 1) as f32),
                        self.array[pos].clone(),
                    )));
                }
            } else {
                let (k, v) = &self.entries[pos - self.array.len()];
                if !v.is_nil() {
                    return Ok(Some((k.0.clone(), v.clone())));
                }
            }
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn array_and_hash() {
        let mut t = Table::default();
        t.set(Value::Number(2.0), Value::str("b")).unwrap();
        t.set(Value::Number(1.0), Value::str("a")).unwrap();
        assert_eq!(t.len(), 2);
        assert_eq!(t.array.len(), 2, "2 migrated into the array part");
        t.set(Value::str("x"), Value::Number(5.0)).unwrap();
        assert_eq!(t.get_str("x").as_number(), Some(5.0));
        let mut keys = Vec::new();
        let mut k = Value::Nil;
        while let Some((nk, _)) = t.next(&k).unwrap() {
            keys.push(nk.to_display());
            k = nk;
        }
        assert_eq!(keys, ["1", "2", "x"]);
        t.set(Value::Number(2.0), Value::Nil).unwrap();
        assert_eq!(t.len(), 1);
        assert!(t.set(Value::Nil, Value::Bool(true)).is_err());
    }
}
