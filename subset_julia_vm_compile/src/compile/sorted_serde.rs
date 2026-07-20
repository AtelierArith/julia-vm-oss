//! Deterministic serializers for hash-based collections.
//!
//! Rust's `HashMap` / `HashSet` iteration order depends on the per-process
//! random hash seed, so naive `serde` serialization produces different bytes
//! for the same logical content across processes. That bites us specifically
//! for `target/base_cache.bin`: `subset_julia_vm/build.rs` declares it as a
//! build dependency (via `include_bytes!`), so any byte difference forces
//! cargo to rebuild `subset_julia_vm` + `subset_julia_vm_web`, and the WASM
//! release profile uses `lto = true` which makes that relink expensive.
//!
//! These helpers emit hash-map / hash-set values in `Ord` key order, matching
//! the on-wire layout that `bincode` produces for any `Map` / `Seq` regardless
//! of insertion order. Deserialization stays the default: bincode reads the
//! length-prefixed entries and reinserts them into a fresh `HashMap` /
//! `HashSet`, so existing in-memory types are unaffected.

use serde::ser::{SerializeMap, SerializeSeq, Serializer};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::hash::Hash;

/// Serialize `HashMap` entries in ascending key order.
pub(crate) fn sorted_hashmap<K, V, S>(map: &HashMap<K, V>, serializer: S) -> Result<S::Ok, S::Error>
where
    K: Serialize + Ord + Hash + Eq,
    V: Serialize,
    S: Serializer,
{
    let mut entries: Vec<(&K, &V)> = map.iter().collect();
    entries.sort_by(|a, b| a.0.cmp(b.0));
    let mut m = serializer.serialize_map(Some(entries.len()))?;
    for (k, v) in entries {
        m.serialize_entry(k, v)?;
    }
    m.end()
}

/// Serialize `HashMap<K, HashSet<T>>` with ascending outer keys and ascending
/// inner elements.
pub(crate) fn sorted_hashmap_of_hashset<K, T, S>(
    map: &HashMap<K, HashSet<T>>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    K: Serialize + Ord + Hash + Eq,
    T: Serialize + Ord + Hash + Eq,
    S: Serializer,
{
    let mut entries: Vec<(&K, &HashSet<T>)> = map.iter().collect();
    entries.sort_by(|a, b| a.0.cmp(b.0));
    let mut m = serializer.serialize_map(Some(entries.len()))?;
    for (k, v) in entries {
        m.serialize_entry(k, &SortedSet(v))?;
    }
    m.end()
}

/// Wrapper that serializes a `HashSet` as a sorted sequence. Bincode treats
/// the wire format identically to a `Vec` of the same length, so deserializing
/// into a `HashSet` (default `Deserialize`) works unchanged.
struct SortedSet<'a, T>(&'a HashSet<T>);

impl<T> Serialize for SortedSet<'_, T>
where
    T: Serialize + Ord + Hash + Eq,
{
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut items: Vec<&T> = self.0.iter().collect();
        items.sort();
        let mut s = serializer.serialize_seq(Some(items.len()))?;
        for item in items {
            s.serialize_element(item)?;
        }
        s.end()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use serde::Serialize;

    #[derive(Serialize)]
    struct Wrap<'a> {
        #[serde(serialize_with = "sorted_hashmap")]
        m: &'a HashMap<String, u32>,
    }

    #[derive(Serialize)]
    struct WrapSet<'a> {
        #[serde(serialize_with = "sorted_hashmap_of_hashset")]
        m: &'a HashMap<String, HashSet<String>>,
    }

    fn key_positions(bytes: &[u8], keys: &[&'static str]) -> Vec<(&'static str, usize)> {
        keys.iter()
            .map(|k| {
                let pos = bytes
                    .windows(k.len())
                    .position(|w| w == k.as_bytes())
                    .unwrap_or_else(|| panic!("key {k} must appear in serialized bytes"));
                (*k, pos)
            })
            .collect()
    }

    #[test]
    fn sorted_hashmap_emits_keys_in_ascending_order() {
        let mut m: HashMap<String, u32> = HashMap::new();
        m.insert("zeta".into(), 1);
        m.insert("alpha".into(), 2);
        m.insert("mu".into(), 3);

        let bytes = bincode::serialize(&Wrap { m: &m }).expect("bincode must succeed");
        let positions = key_positions(&bytes, &["alpha", "mu", "zeta"]);

        assert!(
            positions[0].1 < positions[1].1 && positions[1].1 < positions[2].1,
            "expected ascending key order in bytes, got {positions:?}"
        );
    }

    #[test]
    fn sorted_hashmap_is_invariant_to_insertion_order() {
        let mut a: HashMap<String, u32> = HashMap::new();
        a.insert("z".into(), 1);
        a.insert("a".into(), 2);
        a.insert("m".into(), 3);

        let mut b: HashMap<String, u32> = HashMap::new();
        b.insert("a".into(), 2);
        b.insert("m".into(), 3);
        b.insert("z".into(), 1);

        let bytes_a = bincode::serialize(&Wrap { m: &a }).unwrap();
        let bytes_b = bincode::serialize(&Wrap { m: &b }).unwrap();
        assert_eq!(bytes_a, bytes_b);
    }

    #[test]
    fn sorted_hashmap_of_hashset_orders_inner_elements() {
        let mut inner: HashSet<String> = HashSet::new();
        inner.insert("zulu".into());
        inner.insert("alpha".into());
        inner.insert("mike".into());

        let mut m: HashMap<String, HashSet<String>> = HashMap::new();
        m.insert("group".into(), inner);

        let bytes = bincode::serialize(&WrapSet { m: &m }).expect("bincode must succeed");
        let positions = key_positions(&bytes, &["alpha", "mike", "zulu"]);

        assert!(
            positions[0].1 < positions[1].1 && positions[1].1 < positions[2].1,
            "expected inner HashSet elements in ascending order, got {positions:?}"
        );
    }
}
