//! Owner-scoped struct identity (`StructId`) and the registry that replaces
//! the bare-name `HashMap<String, StructInfo>` compile tables — Issue #11078
//! (Phase 2b of the #10459 semantic-ID epic).
//!
//! # What this retires
//!
//! `SharedCompileContext::struct_table` used to be a `HashMap<String,
//! StructInfo>`: the *display name* was the entry's identity. Registering a
//! module struct under its bare alias (`struct_table.insert_alias("Box", "M.Box")` for
//! `M.Box`, so `Box(...)` works after `using .M`) therefore **destroyed** any
//! same-named entry already there — which is the entire reason
//! `base_struct_table` + `base_origin_bare_names` exist (Issues #10078 /
//! #10257): they keep a private copy of what the alias clobbered.
//!
//! Here, entries are keyed by [`StructId`] and names are *aliases into* that
//! id space. A colliding bare name re-points one alias; it can never remove an
//! entry. `by_name` is the single lexical `name -> id` resolution boundary
//! required by `docs/vm/SEMANTIC_IDENTITIES.md` property #3 ("qualified and
//! bare lookup resolve a name to an ID; they do not use the name as the
//! identity") — the same role `ModuleInternTable`'s own name index plays for
//! `ModuleId` (Issue #10988).
//!
//! # Why `local` is the existing `type_id`, not a fresh counter
//!
//! A concrete struct ALREADY has a dense, program-wide identity: its
//! `StructInfo::type_id`, which the VM uses structurally in bytecode
//! (`NewStruct(type_id)`) and which the Base cache is *built to keep stable*
//! across a fresh compile and a cache restore (Issue #10265's
//! `base_cached_compile_struct_table_matches_fresh_compile` invariant). Minting
//! a second, independently-allocated per-module counter would create a second
//! identity space for the same thing — and its allocation order genuinely
//! DIFFERS between the two lanes (the cached lane seeds every cached
//! `struct_defs` entry, including parametric instantiations such as
//! `Complex{Float64}`, up front; the fresh lane creates those instantiations
//! much later, after the user's own structs), so a naive registration counter
//! would NOT survive the fresh-vs-restore parity requirement.
//!
//! So `StructId` = (owning `ModuleId`, that dense concrete-type index). The
//! *new* information is the OWNER — which is what makes the id owner-scoped —
//! and the id is never derived from `HashMap` iteration order.

use std::collections::{HashMap, HashSet};

use crate::module_intern::{ModuleId, ModuleInternTable, MAIN_MODULE_PATH};
use crate::struct_info::StructInfo;

/// Owner-scoped identity of one concrete struct declaration.
///
/// Two same-named structs declared in different modules (`A.Box` / `B.Box`)
/// always have distinct ids: they differ in `module`, and (because each
/// concrete struct gets its own `type_id`) in `local` too.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StructId {
    module: ModuleId,
    local: u32,
}

impl StructId {
    pub fn new(module: ModuleId, local: u32) -> Self {
        Self { module, local }
    }

    /// The module that DECLARES this struct (not the module a caller happened
    /// to spell its name in).
    pub fn module(self) -> ModuleId {
        self.module
    }

    /// The dense concrete-type index (`StructInfo::type_id`) behind this id.
    pub fn local(self) -> u32 {
        self.local
    }

    /// The concrete-type index this id denotes, i.e. the `StructInfo::type_id`
    /// the VM uses structurally in bytecode (`NewStruct(type_id)`). By
    /// construction `local()` IS that index (see the module docs), so callers
    /// that hold an id never need to re-resolve the entry just to read it.
    pub fn type_id(self) -> usize {
        self.local as usize
    }
}

/// The module path that owns a struct registered under `name`.
///
/// `"M.Sub.Box"` -> `"M.Sub"`; `"Box"` -> `"Main"`. Only the BASE segment (the
/// part before any `{`) is considered, so a type argument's dots never leak
/// into the owner: `"Box{Main.Foo}"` is owned by `Main`, not by `Box{Main`.
pub fn owner_module_path(name: &str) -> &str {
    let base_end = name.find('{').unwrap_or(name.len());
    match name[..base_end].rfind('.') {
        Some(dot) => &name[..dot],
        None => MAIN_MODULE_PATH,
    }
}

/// Compile-side struct table, keyed by owner-scoped [`StructId`].
///
/// Drop-in for the `HashMap<String, StructInfo>` it replaces (`get`,
/// `get_mut`, `insert`, `contains_key`, `iter`, `keys`, `values`, `len`,
/// `is_empty`, and `&registry` iteration all behave the same), plus the
/// id-returning [`StructRegistry::resolve`] that callers use when they need
/// the identity rather than just the layout.
#[derive(Debug, Clone, Default)]
pub struct StructRegistry {
    /// Module-path interning, seeded from the program's module tree so this
    /// registry's `ModuleId`s are the SAME ids `CorePipeline::module_registry`
    /// / `RuntimeCompileContext::module_registry` allocate (one id space, not
    /// two — see `owner_ids_agree_with_program_module_registry` in the compile
    /// crate's tests).
    modules: ModuleInternTable,
    /// The entries themselves. Owner-scoped identity: nothing here is ever
    /// removed or overwritten by a mere name collision.
    entries: HashMap<StructId, StructInfo>,
    /// The single lexical `name -> id` resolution boundary (property #3).
    /// Several names may alias one id (a module struct's qualified name and
    /// its bare `using`-visible alias); a later same-named declaration
    /// re-points the alias without touching either entry.
    by_name: HashMap<String, StructId>,
    /// Canonical declaration lookup. Unlike `by_name`, a bare alias never
    /// writes this index, so a later `using`-visible collision cannot hide the
    /// declaration owned by `Main` or by the current module (Issue #11046).
    by_owner_name: HashMap<(ModuleId, String), StructId>,
    /// The first (declaration) spelling registered for each identity. This
    /// keeps shadowed entries diagnosable and lets type-id consumers enumerate
    /// every declaration rather than only the currently visible aliases.
    canonical_names: HashMap<StructId, String>,
    /// `type_id -> id`, so re-registering an existing struct under another
    /// name is recognized as an ALIAS instead of minting a second identity.
    by_type_id: HashMap<usize, StructId>,
}

impl StructRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// A registry whose module interning is pre-seeded with the program's
    /// module tree (walked in `register_module_ids`' deterministic
    /// depth-first, source-declared order), so owner ids agree with the rest
    /// of the pipeline instead of depending on which struct happens to be
    /// registered first.
    pub fn with_modules(modules: ModuleInternTable) -> Self {
        Self {
            modules,
            ..Self::default()
        }
    }

    /// Register `info` under `name`.
    ///
    /// This registers a declaration, not a lexical alias. Aliases must use
    /// [`StructRegistry::insert_alias`], because `type_id` and layout equality
    /// are not sufficient proof that two same-shaped declarations have the
    /// same owner (Issue #11046).
    ///
    /// # Determinism contract
    ///
    /// `local` is the lane-stable `type_id`, so the only order-sensitive input
    /// is the OWNER's `ModuleId`. Construct the registry with
    /// [`StructRegistry::with_modules`] and a module table seeded by the
    /// program's module-tree walk (`register_module_ids`): then every owner
    /// path is already interned and `insert` allocates nothing, so the ids do
    /// not depend on the order structs are registered in — which is what lets
    /// the cached lane (which seeds `struct_defs` up front) agree with the
    /// fresh lane. An owner path that is NOT in the seeded table is interned
    /// lazily here, which WOULD be registration-order-dependent; production
    /// never hits that (every qualified struct name is qualified BY a module
    /// in the program's tree), and
    /// `struct_registry_owner_paths_are_all_program_modules_11078` pins it.
    pub fn insert(&mut self, name: impl Into<String>, info: StructInfo) {
        let name = name.into();
        let owner_path = owner_module_path(&name).to_string();
        self.insert_owned(name, &owner_path, info);
    }

    /// Register a declaration while carrying its declaring owner
    /// separately from its display spelling. Parametric instantiations may
    /// intentionally retain a bare display name even when a module owns their
    /// declaration; deriving the owner from that spelling would recreate the
    /// name-as-identity bug this registry exists to remove (Issue #11046).
    pub fn insert_owned(&mut self, name: impl Into<String>, owner_path: &str, info: StructInfo) {
        let name = name.into();
        let module = self.modules.intern(owner_path);
        let id = StructId::new(module, info.type_id as u32);
        // Keep the FIRST declaration as the deterministic projection for a
        // legacy `ValueType::Struct(type_id)`. A colliding declaration remains
        // reachable by owner while Issue #11167 retires that lossy carrier.
        self.by_type_id.entry(info.type_id).or_insert(id);
        self.by_owner_name
            .insert((module, local_struct_name(&name).to_string()), id);
        self.canonical_names.insert(id, name.clone());
        self.entries.insert(id, info);
        self.by_name.insert(name, id);
    }

    /// Point a lexical spelling at an existing declaration without claiming
    /// ownership for the alias. The target must be its already-registered
    /// declaration spelling.
    pub fn insert_alias(&mut self, alias: impl Into<String>, target: &str) -> bool {
        let Some(id) = self.by_name.get(target).copied() else {
            return false;
        };
        self.by_name.insert(alias.into(), id);
        true
    }

    /// Resolve a name to the owner-scoped identity AND the layout behind it.
    /// The id-first accessor that name-keyed lookups should migrate to.
    pub fn resolve(&self, name: &str) -> Option<(StructId, &StructInfo)> {
        let owner_path = owner_module_path(name);
        if owner_path != MAIN_MODULE_PATH {
            return self.resolve_in_owner(owner_path, name);
        }
        let id = *self.by_name.get(name)?;
        self.entries.get(&id).map(|info| (id, info))
    }

    /// Resolve a declaration by its owning module and local type spelling.
    /// Bare aliases are deliberately excluded: ownership is structural, not
    /// whichever name happened to win the lexical alias slot most recently.
    pub fn resolve_in_owner(
        &self,
        owner_path: &str,
        name: &str,
    ) -> Option<(StructId, &StructInfo)> {
        let module = self.modules.lookup(owner_path)?;
        let id = *self
            .by_owner_name
            .get(&(module, local_struct_name(name).to_string()))?;
        self.entries.get(&id).map(|info| (id, info))
    }

    /// Resolve a type spelling at a compiler scope boundary.
    ///
    /// Qualified names are exact. Bare names first select a declaration owned
    /// by the current module, then (for Base/prelude compilation) the preserved
    /// `Main` declaration, and finally the ordinary lexical alias installed by
    /// `using`. This replaces the parallel `base_struct_table` workaround that
    /// existed only because the old name-keyed table destroyed shadowed rows.
    pub fn resolve_scoped(
        &self,
        name: &str,
        current_module_path: Option<&str>,
        prefer_main_owner: bool,
    ) -> Option<(StructId, &StructInfo)> {
        if owner_module_path(name) != MAIN_MODULE_PATH {
            return self.resolve(name);
        }
        if let Some(owner_path) = current_module_path {
            if let Some(resolved) = self.resolve_in_owner(owner_path, name) {
                return Some(resolved);
            }
        }
        if prefer_main_owner {
            if let Some(resolved) = self.resolve_in_owner(MAIN_MODULE_PATH, name) {
                return Some(resolved);
            }
        }
        self.resolve(name)
    }

    /// The id `name` currently resolves to, if any.
    pub fn id_of(&self, name: &str) -> Option<StructId> {
        self.by_name.get(name).copied()
    }

    /// The entry behind an id. Unlike a name lookup this CANNOT be shadowed by
    /// a later same-named declaration — that is the point of the id.
    pub fn info_by_id(&self, id: StructId) -> Option<&StructInfo> {
        self.entries.get(&id)
    }

    /// Deterministically project the legacy dense type id to the first
    /// declaration registered for it. This is containment for Issue #11167;
    /// callers must not scan the unordered owner registry and pick an arbitrary
    /// same-id layout.
    pub fn resolve_type_id(&self, type_id: usize) -> Option<(StructId, &String, &StructInfo)> {
        let id = *self.by_type_id.get(&type_id)?;
        let name = self.canonical_names.get(&id)?;
        let info = self.entries.get(&id)?;
        Some((id, name, info))
    }

    pub fn get(&self, name: &str) -> Option<&StructInfo> {
        let id = self.by_name.get(name)?;
        self.entries.get(id)
    }

    pub fn get_mut(&mut self, name: &str) -> Option<&mut StructInfo> {
        let id = *self.by_name.get(name)?;
        self.entries.get_mut(&id)
    }

    pub fn contains_key(&self, name: &str) -> bool {
        self.by_name.contains_key(name)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &StructInfo)> + '_ {
        self.by_name
            .iter()
            .filter_map(|(name, id)| self.entries.get(id).map(|info| (name, info)))
    }

    pub fn keys(&self) -> impl Iterator<Item = &String> + '_ {
        self.by_name.keys()
    }

    pub fn values(&self) -> impl Iterator<Item = &StructInfo> + '_ {
        self.iter().map(|(_, info)| info)
    }

    /// Number of NAMES resolvable in this registry (aliases included), i.e.
    /// what the replaced `HashMap<String, StructInfo>::len()` reported.
    pub fn len(&self) -> usize {
        self.by_name.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_name.is_empty()
    }

    /// Number of distinct struct DECLARATIONS, which is `len()` minus the
    /// aliases. `entry_count() < len()` exactly when some struct is reachable
    /// under both a qualified name and a bare alias.
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    /// Every entry by id, for identity-level assertions (an entry that a bare
    /// alias has shadowed is still here — it is NOT reachable by that name).
    pub fn entries(&self) -> impl Iterator<Item = (StructId, &StructInfo)> + '_ {
        self.entries.iter().map(|(id, info)| (*id, info))
    }

    /// Every declaration with its canonical name and layout, including rows
    /// whose bare lexical alias has since been shadowed.
    pub fn canonical_entries(&self) -> impl Iterator<Item = (StructId, &String, &StructInfo)> + '_ {
        self.canonical_names
            .iter()
            .filter_map(|(id, name)| self.entries.get(id).map(|info| (*id, name, info)))
    }

    /// Project an id back to its owning module path (property #5: diagnostics
    /// map ids back to module-qualified user-facing names).
    pub fn owner_path(&self, id: StructId) -> Option<&str> {
        self.modules.path(id.module())
    }

    pub fn module_registry(&self) -> &ModuleInternTable {
        &self.modules
    }

    /// Remove every declaration at or above a discarded dense type-id suffix.
    ///
    /// All name/owner/canonical projections are filtered from the same retained
    /// identity set so no alias can resurrect an unreached REPL declaration
    /// after the compiled snapshot is checkpointed (Issues #9784/#11546).
    pub fn retain_type_ids_below(&mut self, cutoff: usize) {
        self.entries.retain(|_, info| info.type_id < cutoff);
        let retained: HashSet<StructId> = self.entries.keys().copied().collect();
        self.by_name.retain(|_, id| retained.contains(id));
        self.by_owner_name.retain(|_, id| retained.contains(id));
        self.canonical_names.retain(|id, _| retained.contains(id));
        self.by_type_id
            .retain(|type_id, id| *type_id < cutoff && retained.contains(id));
    }
}

fn local_struct_name(name: &str) -> &str {
    let base_end = name.find('{').unwrap_or(name.len());
    match name[..base_end].rfind('.') {
        Some(dot) => &name[dot + 1..],
        None => name,
    }
}

impl<'a> IntoIterator for &'a StructRegistry {
    type Item = (&'a String, &'a StructInfo);
    type IntoIter = Box<dyn Iterator<Item = (&'a String, &'a StructInfo)> + 'a>;

    fn into_iter(self) -> Self::IntoIter {
        Box::new(self.iter())
    }
}

impl FromIterator<(String, StructInfo)> for StructRegistry {
    fn from_iter<T: IntoIterator<Item = (String, StructInfo)>>(iter: T) -> Self {
        let mut registry = Self::new();
        for (name, info) in iter {
            registry.insert(name, info);
        }
        registry
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ValueType;

    fn info(type_id: usize) -> StructInfo {
        StructInfo {
            type_id,
            is_mutable: false,
            fields: vec![("x".to_string(), ValueType::I64)],
            has_inner_constructor: false,
        }
    }

    #[test]
    fn owner_module_path_uses_the_base_segment_only_issue_11078() {
        assert_eq!(owner_module_path("Box"), "Main");
        assert_eq!(owner_module_path("M.Box"), "M");
        assert_eq!(owner_module_path("Outer.Inner.Box"), "Outer.Inner");
        // A dot inside the type arguments is NOT an owner qualification.
        assert_eq!(owner_module_path("Box{Main.Foo}"), "Main");
        assert_eq!(owner_module_path("M.Box{Int64}"), "M");
    }

    #[test]
    fn sibling_module_same_named_structs_get_distinct_ids_issue_11078() {
        let mut reg = StructRegistry::new();
        reg.insert("A.Box", info(10));
        reg.insert("B.Box", info(11));

        let (a_id, _) = reg.resolve("A.Box").expect("A.Box registered");
        let (b_id, _) = reg.resolve("B.Box").expect("B.Box registered");
        assert_ne!(a_id, b_id);
        assert_ne!(a_id.module(), b_id.module());
        assert_eq!(reg.owner_path(a_id), Some("A"));
        assert_eq!(reg.owner_path(b_id), Some("B"));
    }

    #[test]
    fn a_bare_alias_is_the_same_identity_not_a_second_one_issue_11078() {
        let mut reg = StructRegistry::new();
        reg.insert("M.Box", info(7));
        assert!(reg.insert_alias("Box", "M.Box"));

        let (qualified, _) = reg.resolve("M.Box").expect("qualified");
        let (bare, _) = reg.resolve("Box").expect("bare alias");
        assert_eq!(qualified, bare);
        assert_eq!(
            reg.entry_count(),
            1,
            "an alias must not mint a second entry"
        );
        assert_eq!(reg.len(), 2, "but both names resolve");
        // The alias does not re-own the struct: M declared it, not Main.
        assert_eq!(reg.owner_path(qualified), Some("M"));
    }

    /// The load-bearing claim of Issue #11078: a colliding bare-name alias
    /// SHADOWS, it does not DESTROY. This is what a `HashMap<String,
    /// StructInfo>` could not do, and it is the precondition for eventually
    /// retiring `base_struct_table`/`base_origin_bare_names` (#10078/#10257).
    #[test]
    fn a_clobbering_bare_alias_shadows_but_never_removes_an_entry_issue_11078() {
        let mut reg = StructRegistry::new();
        // A top-level (Base-origin) struct owns the bare name first.
        reg.insert("Box", info(3));
        let (toplevel, _) = reg.resolve("Box").expect("top-level Box");

        // A module declares its own `Box` and re-points the bare alias, exactly
        // as `build_struct_tables` does for `using`-visibility.
        reg.insert("M.Box", info(9));
        assert!(reg.insert_alias("Box", "M.Box"));

        let (shadowing, _) = reg.resolve("Box").expect("bare name still resolves");
        assert_ne!(shadowing, toplevel, "the bare name now resolves to M.Box");
        assert_eq!(reg.owner_path(shadowing), Some("M"));

        // ...and the clobbered entry is STILL THERE, by id.
        let preserved = reg
            .info_by_id(toplevel)
            .expect("the shadowed top-level entry survives the collision");
        assert_eq!(preserved.type_id, 3);
        assert_eq!(reg.owner_path(toplevel), Some("Main"));
        assert_eq!(reg.entry_count(), 2, "two declarations, two entries");
    }

    #[test]
    fn scoped_resolution_recovers_owner_after_bare_alias_collision_issue_11046() {
        let mut modules = ModuleInternTable::new();
        modules.intern("M");
        let mut reg = StructRegistry::with_modules(modules);
        reg.insert("Partition", info(3));
        reg.insert("M.Partition", info(9));
        assert!(reg.insert_alias("Partition", "M.Partition"));

        assert_eq!(
            reg.resolve("Partition").map(|(_, info)| info.type_id),
            Some(9)
        );
        assert_eq!(
            reg.resolve_scoped("Partition", None, true)
                .map(|(_, info)| info.type_id),
            Some(3),
        );
        assert_eq!(
            reg.resolve_scoped("Partition", Some("M"), true)
                .map(|(_, info)| info.type_id),
            Some(9),
        );
        assert_eq!(
            reg.resolve_scoped("M.Partition", None, true)
                .map(|(_, info)| info.type_id),
            Some(9),
        );
    }

    #[test]
    fn qualified_resolution_ignores_lexical_alias_overwrite_issue_11469() {
        let mut reg = StructRegistry::new();
        reg.insert("A.Box", info(7));
        reg.insert("B.Box", info(9));

        // Simulate a later lexical alias accidentally occupying a qualified
        // spelling. Qualified resolution remains structural by owner.
        assert!(reg.insert_alias("A.Box", "B.Box"));
        assert_eq!(reg.resolve("A.Box").map(|(_, info)| info.type_id), Some(7));
        assert_eq!(reg.get("A.Box").map(|info| info.type_id), Some(9));
    }

    #[test]
    fn aliases_never_claim_main_ownership_issue_11046() {
        let mut modules = ModuleInternTable::new();
        modules.intern("M");
        let mut reg = StructRegistry::with_modules(modules);
        reg.insert("M.Box", info(7));
        assert!(reg.insert_alias("Box", "M.Box"));

        assert!(reg.resolve_in_owner("Main", "Box").is_none());
        assert_eq!(
            reg.resolve_in_owner("M", "Box")
                .map(|(_, info)| info.type_id),
            Some(7),
        );
    }

    #[test]
    fn bare_display_name_can_carry_a_module_owner_issue_11046() {
        let mut modules = ModuleInternTable::new();
        modules.intern("M");
        let mut reg = StructRegistry::with_modules(modules);
        reg.insert_owned("Box{Int64}", "M", info(12));

        let (id, _) = reg.resolve("Box{Int64}").expect("bare display alias");
        assert_eq!(reg.owner_path(id), Some("M"));
        assert_eq!(
            reg.resolve_in_owner("M", "Box{Int64}")
                .map(|(_, info)| info.type_id),
            Some(12),
        );
        assert!(reg.resolve_in_owner("Main", "Box{Int64}").is_none());
    }

    #[test]
    fn owner_index_preserves_concrete_type_arguments_issue_11046() {
        let mut modules = ModuleInternTable::new();
        modules.intern("M");
        let mut reg = StructRegistry::with_modules(modules);
        reg.insert_owned("Box{Int64}", "M", info(12));
        reg.insert_owned("Box{Float64}", "M", info(13));

        assert_eq!(
            reg.resolve_in_owner("M", "Box{Int64}")
                .map(|(_, info)| info.type_id),
            Some(12),
        );
        assert_eq!(
            reg.resolve_in_owner("M", "Box{Float64}")
                .map(|(_, info)| info.type_id),
            Some(13),
        );
        assert!(reg.resolve_in_owner("M", "Box{String}").is_none());
    }

    #[test]
    fn same_layout_and_type_id_do_not_alias_across_owners_issue_11046() {
        let mut modules = ModuleInternTable::new();
        modules.intern("A");
        modules.intern("B");
        let mut reg = StructRegistry::with_modules(modules);
        reg.insert_owned("A.Box", "A", info(7));
        reg.insert_owned("B.Box", "B", info(7));

        let a = reg.resolve_in_owner("A", "Box").map(|(id, _)| id);
        let b = reg.resolve_in_owner("B", "Box").map(|(id, _)| id);
        assert!(a.is_some());
        assert!(b.is_some());
        assert_ne!(a, b);
    }

    /// The determinism contract, stated as a test: with the module table
    /// seeded from the program's module tree (what production does), the ids
    /// are independent of the order structs are registered in. This is the
    /// property the cached lane needs — it seeds every cached `struct_defs`
    /// entry up front, in a different order from the fresh lane — and the one
    /// a per-registry allocation counter would NOT have.
    #[test]
    fn ids_do_not_depend_on_insertion_order_when_modules_are_seeded_issue_11078() {
        let seed = || {
            let mut modules = ModuleInternTable::new();
            modules.intern("A");
            modules.intern("B");
            StructRegistry::with_modules(modules)
        };

        let mut forward = seed();
        forward.insert("A.Box", info(10));
        forward.insert("B.Box", info(11));

        let mut reverse = seed();
        reverse.insert("B.Box", info(11));
        reverse.insert("A.Box", info(10));

        assert_eq!(forward.id_of("A.Box"), reverse.id_of("A.Box"));
        assert_eq!(forward.id_of("B.Box"), reverse.id_of("B.Box"));
    }

    /// Negative control for the contract above: WITHOUT a seeded module table,
    /// owner ids are minted in struct-registration order, so the ids DO drift.
    /// Pinned so nobody "simplifies" `with_modules` away without noticing that
    /// it is load-bearing for fresh-vs-cache-restore parity.
    #[test]
    fn unseeded_module_interning_is_registration_order_dependent_issue_11078() {
        let mut forward = StructRegistry::new();
        forward.insert("A.Box", info(10));
        forward.insert("B.Box", info(11));

        let mut reverse = StructRegistry::new();
        reverse.insert("B.Box", info(11));
        reverse.insert("A.Box", info(10));

        assert_ne!(
            forward.id_of("A.Box"),
            reverse.id_of("A.Box"),
            "an unseeded registry mints owner ids lazily, in registration order"
        );
    }

    /// Containment for Issue #11167 (found by an adversarial codex review of the
    /// #11078 PR): the dense `type_id` allocator can hand ONE slot to two
    /// DIFFERENT declarations (a module struct whose bare name collides with a
    /// Base parametric family overwrites that family's `struct_defs` row). The
    /// registry must NOT then alias them into a single entry — the later one
    /// would silently replace the earlier one's layout for every reader of the
    /// earlier name, including a `base_struct_table` id, which before #11078 held
    /// a private immutable clone. Same `type_id` + DIFFERENT layout = two
    /// declarations, kept distinct (their owner-scoped ids differ).
    #[test]
    fn same_type_id_with_a_different_layout_is_not_aliased_issue_11167() {
        let mut modules = ModuleInternTable::new();
        modules.intern("M");
        let mut reg = StructRegistry::with_modules(modules);

        // Base's `Complex` holds slot 0 ...
        let base = StructInfo {
            type_id: 0,
            is_mutable: false,
            fields: vec![("re".to_string(), ValueType::F64)],
            has_inner_constructor: false,
        };
        reg.insert("Complex", base.clone());
        let Some(base_id) = reg.id_of("Complex") else {
            unreachable!("Complex registered")
        };

        // ... and the buggy allocator hands the SAME slot to `M.Complex`, whose
        // layout is different.
        let module_struct = StructInfo {
            type_id: 0,
            is_mutable: false,
            fields: vec![("x".to_string(), ValueType::I64)],
            has_inner_constructor: false,
        };
        reg.insert("M.Complex", module_struct.clone());
        let Some(module_id) = reg.id_of("M.Complex") else {
            unreachable!("M.Complex registered")
        };

        assert_ne!(
            base_id, module_id,
            "two different declarations must not collapse into one entry"
        );
        assert_eq!(reg.entry_count(), 2);
        // The earlier declaration's layout is INTACT — this is the regression the
        // guard prevents.
        assert_eq!(reg.info_by_id(base_id), Some(&base));
        assert_eq!(reg.info_by_id(module_id), Some(&module_struct));
        assert_eq!(reg.owner_path(base_id), Some("Main"));
        assert_eq!(reg.owner_path(module_id), Some("M"));
        assert_eq!(
            reg.resolve_type_id(0).map(|(id, _, _)| id),
            Some(base_id),
            "legacy type-id projection must select the first declaration deterministically",
        );
    }

    #[test]
    fn get_mut_reaches_the_entry_behind_an_alias_issue_11078() {
        let mut reg = StructRegistry::new();
        reg.insert("M.Box", info(4));
        assert!(reg.insert_alias("Box", "M.Box"));

        if let Some(entry) = reg.get_mut("Box") {
            entry.is_mutable = true;
        }
        // Both names see it: they are one entry.
        assert!(reg.get("M.Box").expect("qualified").is_mutable);
    }

    #[test]
    fn retaining_type_id_prefix_removes_every_suffix_projection_issue_9784() {
        let mut reg = StructRegistry::new();
        reg.insert("Kept", info(4));
        reg.insert("M.Dropped", info(5));
        assert!(reg.insert_alias("Dropped", "M.Dropped"));

        let dropped_id = reg.id_of("M.Dropped").expect("suffix id");
        reg.retain_type_ids_below(5);

        assert_eq!(reg.get("Kept").map(|entry| entry.type_id), Some(4));
        assert!(reg.get("M.Dropped").is_none());
        assert!(reg.get("Dropped").is_none());
        assert!(reg.info_by_id(dropped_id).is_none());
        assert!(reg.resolve_type_id(5).is_none());
        assert!(reg
            .canonical_entries()
            .all(|(_, _, entry)| entry.type_id < 5));
    }
}
