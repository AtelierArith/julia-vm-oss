//! Interned module identity (`ModuleId`) — Issue #10988 Phase 2a foundation.
//!
//! `docs/vm/SEMANTIC_ID_MIGRATION.md` (Issue #10459 Phase 0) inventories 117
//! `module`/`global`-domain bare-name identity sites. `docs/vm/SEMANTIC_IDENTITIES.md`'s
//! target model has `StructId { module: ModuleId, ... }` and
//! `FunctionId { module: ModuleId, ... }` both embed a `ModuleId`, so it must
//! exist — allocated by module *registration order*, never by `HashMap`
//! iteration order — before Phase 2b (`StructId`) / Phase 3 (`FunctionId`)
//! can type their ids correctly. This module is that foundation.
//!
//! # Design precedent
//!
//! `ConcreteTypeId` / `TypeInternTable` (`type_intern.rs`,
//! `docs/vm/TYPE_INTERNING.md`) is the closest existing precedent — a
//! session-scoped interned id with a lookup table — but modules are not
//! dispatch types, so this is a distinct type, not a variant of
//! `ConcreteTypeKey` (mirrors the S7 `MethodTableKey` reasoning: a generic
//! function name is not a concrete type either, and folding it into the type
//! registry would be a semantic mismatch).
//!
//! # Allocation order
//!
//! `ModuleId(0)` is always `"Main"` (pre-interned by [`ModuleInternTable::new`]).
//! Every other id is allocated the first time a fully-qualified module path
//! (`"Main"`, `"Dates"`, `"AbstractAlgebra.Integers"`, ...) is registered.
//! Production callers register modules by walking the `Program`'s module tree
//! in its stable, source-declared depth-first order (`compile/collect.rs`'s
//! `register_module_ids`, which mirrors `collect_module_info`'s own
//! recursion) — **never** by iterating an already-built `HashMap<String, _>`,
//! whose iteration order is per-process-random (`RandomState`) and would make
//! two fresh compiles of the *same* source allocate different ids.
//!
//! # Cache/serialization
//!
//! Unlike `TypeInternTable` (runtime-only, never serialized),
//! `ModuleInternTable` is `Serialize`/`Deserialize` and — for the migrated
//! `CompiledProgram::macro_bindings` consumer (Issue #10988) — travels through
//! the persisted Base cache / `.sjvmbc`/`.sjir` wire format alongside the table
//! it keys. See `docs/vm/CACHE_ARCHITECTURE.md`'s "Owner-scoped id relocation"
//! section for the full relocation-pattern writeup (Issue #10988/#10991).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// The always-present root module, pre-interned as id 0 by every
/// [`ModuleInternTable::new`] so `Main`'s id is stable across sessions without
/// depending on registration order for that one entry.
pub const MAIN_MODULE_PATH: &str = "Main";

/// Interned identity of a Julia module within one compiled program /
/// VM session. A dense `u32` handle, allocated in module-registration order
/// (see the module docs) and never reused: two distinct fully-qualified
/// module paths (including same-named submodules of different parents, e.g.
/// `"A.Sub"` vs `"B.Sub"`) always get distinct ids, and re-registering an
/// already-known path returns its existing id unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ModuleId(u32);

impl ModuleId {
    /// The raw dense index behind this id. Exposed for diagnostics/tests; not
    /// meant for arithmetic — compare/hash the `ModuleId` itself.
    #[inline]
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// Session-scoped module-path <-> [`ModuleId`] interning table.
///
/// `paths[id.index()]` is the fully-qualified path that id was interned for
/// (round-trip / display / diagnostics — property 5 of `SEMANTIC_IDENTITIES.md`'s
/// target model: "Diagnostics project IDs back to module-qualified user-facing
/// names"); `index` is the reverse lookup used by `intern`/`lookup`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleInternTable {
    paths: Vec<String>,
    index: HashMap<String, ModuleId>,
}

impl ModuleInternTable {
    /// A fresh table with `"Main"` pre-interned as [`ModuleId`] `0`.
    pub fn new() -> Self {
        let mut table = Self {
            paths: Vec::new(),
            index: HashMap::new(),
        };
        table.intern(MAIN_MODULE_PATH);
        table
    }

    /// Register `path` if not already known and return its `ModuleId`.
    /// Idempotent: re-interning an already-known path returns the SAME id it
    /// was first assigned, never a new one.
    pub fn intern(&mut self, path: &str) -> ModuleId {
        if let Some(&id) = self.index.get(path) {
            return id;
        }
        let id = ModuleId(self.paths.len() as u32);
        self.paths.push(path.to_string());
        self.index.insert(path.to_string(), id);
        id
    }

    /// Look up an already-registered path's id without allocating a new one.
    pub fn lookup(&self, path: &str) -> Option<ModuleId> {
        self.index.get(path).copied()
    }

    /// The fully-qualified path an id was interned for (the inverse of
    /// `intern`/`lookup`), for diagnostics that must project an id back to a
    /// user-facing module-qualified name.
    pub fn path(&self, id: ModuleId) -> Option<&str> {
        self.paths.get(id.index()).map(String::as_str)
    }

    /// Number of distinct modules registered (always >= 1: `Main`).
    pub fn len(&self) -> usize {
        self.paths.len()
    }

    pub fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }

    /// Every registered path, in `ModuleId` (registration) order. For
    /// diagnostics/enumeration (e.g. fresh-vs-cache-restore parity checks);
    /// production lookups should use `intern`/`lookup`/`path`, not scan this.
    pub fn paths(&self) -> impl Iterator<Item = &str> + '_ {
        self.paths.iter().map(String::as_str)
    }
}

impl Default for ModuleInternTable {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn main_is_preinterned_as_id_zero_issue_10988() {
        let table = ModuleInternTable::new();
        assert_eq!(table.lookup("Main"), Some(ModuleId(0)));
        assert_eq!(table.path(ModuleId(0)), Some("Main"));
        assert_eq!(table.len(), 1);
    }

    #[test]
    fn intern_is_idempotent_issue_10988() {
        let mut table = ModuleInternTable::new();
        let a1 = table.intern("Dates");
        let a2 = table.intern("Dates");
        assert_eq!(a1, a2);
        assert_eq!(table.len(), 2);
    }

    #[test]
    fn registration_order_not_hashmap_order_issue_10988() {
        // Ids are assigned strictly in first-seen (registration) order, so
        // interning in a fixed sequence always reproduces the same id
        // sequence, unlike deriving ids from HashMap iteration (per-process
        // random order via RandomState).
        let mut table = ModuleInternTable::new();
        let dates = table.intern("Dates");
        let stats = table.intern("Statistics");
        let dates_again = table.intern("Dates");
        assert_eq!(dates, dates_again);
        assert!(dates.index() < stats.index());
        assert_eq!(dates.index(), 1); // right after Main (0)
        assert_eq!(stats.index(), 2);
    }

    #[test]
    fn same_name_different_module_gets_distinct_ids_issue_10988() {
        // Two sibling submodules named identically under different parents
        // (`A.Sub` / `B.Sub`) must never collide: the registration key is the
        // fully-qualified path, not the bare local name.
        let mut table = ModuleInternTable::new();
        let a_sub = table.intern("A.Sub");
        let b_sub = table.intern("B.Sub");
        assert_ne!(a_sub, b_sub);
        assert_eq!(table.path(a_sub), Some("A.Sub"));
        assert_eq!(table.path(b_sub), Some("B.Sub"));
    }

    #[test]
    fn lookup_does_not_allocate_issue_10988() {
        let mut table = ModuleInternTable::new();
        table.intern("Dates");
        assert!(table.lookup("NeverRegistered").is_none());
        assert_eq!(table.len(), 2);
    }

    #[test]
    fn serde_round_trip_preserves_ids_and_paths_issue_10988() {
        let mut table = ModuleInternTable::new();
        table.intern("Dates");
        table.intern("AbstractAlgebra.Integers");
        let bytes = bincode::serialize(&table).expect("serialize");
        let restored: ModuleInternTable = bincode::deserialize(&bytes).expect("deserialize");
        assert_eq!(restored.lookup("Dates"), table.lookup("Dates"));
        assert_eq!(
            restored.lookup("AbstractAlgebra.Integers"),
            table.lookup("AbstractAlgebra.Integers")
        );
        assert_eq!(restored.path(ModuleId(0)), Some("Main"));
    }
}
