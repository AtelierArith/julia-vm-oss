//! Thread-local runtime registry for `@enum` types (Issue #5139).
//!
//! `Value::Enum { type_name, value }` deliberately carries only the integer
//! value (its layout is matched against in ~30 sites and must stay compact).
//! The member *name* and the per-type declaration order are looked up here
//! instead, mirroring the `SymbolValue` interner pattern in `macro_.rs`.
//!
//! The registry is populated by the `Instr::RegisterEnum` instruction the
//! compiler emits once per `@enum` definition (before binding the members),
//! and consulted by:
//! - value display (`vm::formatting`) — render `red` rather than `Color(0)`,
//! - `Color(value)` construction — validate the integer against known members,
//! - `instances(Color)` — return members in declaration order.

use std::cell::RefCell;
use std::collections::HashMap;

/// One registered enum type: its members in declaration order.
#[derive(Debug, Clone, Default)]
struct EnumTypeEntry {
    /// `(member_name, value)` in declaration order.
    members: Vec<(String, i64)>,
    /// `value -> member_name` for O(1) display / construction lookups.
    by_value: HashMap<i64, String>,
}

thread_local! {
    /// `type_name -> EnumTypeEntry`. Thread-local so it follows the same
    /// single-threaded VM model as the rest of the runtime registries.
    static ENUM_REGISTRY: RefCell<HashMap<String, EnumTypeEntry>> =
        RefCell::new(HashMap::new());
}

/// Cloneable point-in-time image of the thread-local enum registry.
#[derive(Debug, Clone)]
pub struct EnumRegistrySnapshot(HashMap<String, EnumTypeEntry>);

/// Capture the current thread's complete enum registry.
pub fn snapshot() -> EnumRegistrySnapshot {
    ENUM_REGISTRY.with(|cell| EnumRegistrySnapshot(cell.borrow().clone()))
}

/// Replace the current thread's enum registry with a prior snapshot.
pub fn restore(snapshot: EnumRegistrySnapshot) {
    ENUM_REGISTRY.with(|cell| *cell.borrow_mut() = snapshot.0);
}

/// RAII owner for a tentative enum-registry mutation. Dropping an uncommitted
/// transaction restores the complete prior registry; `commit` keeps all
/// mutations performed since `begin` (Issue #9784).
#[derive(Debug)]
#[must_use = "dropping an uncommitted enum transaction rolls it back"]
pub struct EnumRegistryTransaction {
    snapshot: Option<EnumRegistrySnapshot>,
}

impl EnumRegistryTransaction {
    pub fn begin() -> Self {
        Self {
            snapshot: Some(snapshot()),
        }
    }

    pub fn commit(mut self) {
        self.snapshot = None;
    }
}

impl Drop for EnumRegistryTransaction {
    fn drop(&mut self) {
        if let Some(snapshot) = self.snapshot.take() {
            restore(snapshot);
        }
    }
}

/// Register (or replace) an enum type and its members. Idempotent: re-running
/// the same `@enum` definition (e.g. REPL replay) simply overwrites the entry.
pub fn register_enum(type_name: &str, members: &[(String, i64)]) {
    let mut by_value = HashMap::with_capacity(members.len());
    for (name, value) in members {
        // First declaration of a value wins for the display name, matching
        // upstream where the canonical printed name is the first member with
        // that value.
        by_value.entry(*value).or_insert_with(|| name.clone());
    }
    let entry = EnumTypeEntry {
        members: members.to_vec(),
        by_value,
    };
    ENUM_REGISTRY.with(|cell| {
        cell.borrow_mut().insert(type_name.to_string(), entry);
    });
}

/// Whether `type_name` names a registered enum type.
pub fn is_registered_enum(type_name: &str) -> bool {
    ENUM_REGISTRY.with(|cell| cell.borrow().contains_key(type_name))
}

/// The display name for an enum member, or `None` if the value is unknown.
pub fn member_name(type_name: &str, value: i64) -> Option<String> {
    ENUM_REGISTRY.with(|cell| {
        cell.borrow()
            .get(type_name)
            .and_then(|e| e.by_value.get(&value).cloned())
    })
}

/// The members of an enum type in declaration order, or `None` if unregistered.
pub fn members(type_name: &str) -> Option<Vec<(String, i64)>> {
    ENUM_REGISTRY.with(|cell| cell.borrow().get(type_name).map(|e| e.members.clone()))
}

/// Whether `value` is a valid member value of the (registered) enum.
pub fn is_valid_value(type_name: &str, value: i64) -> bool {
    ENUM_REGISTRY.with(|cell| {
        cell.borrow()
            .get(type_name)
            .map(|e| e.by_value.contains_key(&value))
            .unwrap_or(false)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_and_lookup() {
        register_enum(
            "Color",
            &[
                ("red".to_string(), 0),
                ("green".to_string(), 1),
                ("blue".to_string(), 2),
            ],
        );
        assert!(is_registered_enum("Color"));
        assert!(!is_registered_enum("Nope"));
        assert_eq!(member_name("Color", 1).as_deref(), Some("green"));
        assert_eq!(member_name("Color", 9), None);
        assert!(is_valid_value("Color", 2));
        assert!(!is_valid_value("Color", 7));
        let m = members("Color").unwrap();
        assert_eq!(m[0], ("red".to_string(), 0));
        assert_eq!(m[2], ("blue".to_string(), 2));
    }

    #[test]
    fn uncommitted_transaction_restores_members_and_display_lookup_9784() {
        register_enum(
            "RollbackEnum9784",
            &[
                ("old_first9784".to_string(), 7),
                ("old_second9784".to_string(), 8),
            ],
        );
        {
            let _transaction = EnumRegistryTransaction::begin();
            register_enum(
                "RollbackEnum9784",
                &[
                    ("new_first9784".to_string(), 8),
                    ("new_second9784".to_string(), 7),
                ],
            );
            assert_eq!(
                member_name("RollbackEnum9784", 7).as_deref(),
                Some("new_second9784")
            );
        }

        assert_eq!(
            members("RollbackEnum9784"),
            Some(vec![
                ("old_first9784".to_string(), 7),
                ("old_second9784".to_string(), 8)
            ])
        );
        assert_eq!(
            member_name("RollbackEnum9784", 7).as_deref(),
            Some("old_first9784")
        );
    }

    #[test]
    fn committed_transaction_preserves_new_registry_9784() {
        register_enum("CommittedEnum9784", &[("before_commit9784".to_string(), 0)]);
        let transaction = EnumRegistryTransaction::begin();
        register_enum("CommittedEnum9784", &[("after_commit9784".to_string(), 1)]);
        transaction.commit();

        assert_eq!(
            members("CommittedEnum9784"),
            Some(vec![("after_commit9784".to_string(), 1)])
        );
        assert_eq!(
            member_name("CommittedEnum9784", 1).as_deref(),
            Some("after_commit9784")
        );
    }
}
