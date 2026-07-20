//! Precise inference backedges with stable specialization identity
//! (Issue #8553, slice 1/3 of Issue #8442).
//!
//! Upstream Julia stores, per inferred `CodeInstance`, the exact set of
//! `MethodInstance` edges the result depends on (`store_backedges` in
//! `julia/Compiler/src/typeinfer.jl`) plus per-`GlobalRef` binding edges
//! (`julia/Compiler/src/bindinginvalidations.jl`), and stamps each entry with
//! a `WorldRange` (`julia/Compiler/src/cicache.jl`). Invalidation then walks
//! backedges from the mutated method/binding instead of scanning every cache.
//!
//! sjulia's engine so far keeps only name-keyed approximations
//! (`function_dependencies`, `method_dependencies`, `global_binding_dependencies`
//! plus the PR #8526 name → cache-key index). This module adds the *precise*
//! graph those approximations stand in for:
//!
//! - [`MethodKey`] — stable method identity (≈ upstream `Method`): the
//!   generic-function name plus the canonical declared signature
//!   (`Tuple{...}` wrapped in one `UnionAll` per `where` parameter) and the
//!   vararg shape. It is derived from the definition itself — never from
//!   method-table insertion order — so it survives re-compilation.
//! - [`SpecializationKey`] — stable specialization identity (≈ upstream
//!   `MethodInstance`): a [`MethodKey`] plus the canonical specialized
//!   argument signature as a [`CoreType`] tuple.
//! - [`BackedgeIndex`] — the recorded graph: per caller specialization, the
//!   resolved call edges (`(callee method, call argtypes, kind)`) and global
//!   binding reads, with reverse name/binding indexes for the invalidation
//!   walk, and a per-specialization [`WorldRange`] stamp.
//!
//! Since Issue #8554 (slice 2/3) this graph is also the production
//! invalidation source: a method/binding mutation seeds the affected
//! specializations here and walks the reverse indexes transitively
//! ([`BackedgeIndex::method_mutation_seeds`],
//! [`BackedgeIndex::binding_mutation_seeds`],
//! [`BackedgeIndex::transitively_affected`]), while cache entries not covered
//! by the graph keep the conservative name-keyed decision. The index is
//! in-memory only — it is never serialized into the persisted Base/IPO
//! caches, so it does not participate in the #8444 cache schema fingerprint
//! and cannot change the wire format.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::rc::Rc;

use crate::compile::lattice::types::{ConstValue, LatticeType};
use crate::compile::method_table::{MethodSig, ReplMethodIdentity};
#[cfg(test)]
use crate::inference_core::CoreTypeVar;
use crate::inference_core::{CorePrimitive, CoreType, CoreValueParam};
use crate::ir::core::Function;

use super::cache_key::CacheArgType;
use super::world::{World, WorldRange};

/// Stable method identity (≈ upstream `Method`).
///
/// Two definitions with the same name, canonical declared signature, and
/// vararg shape denote the same method slot (a redefinition replaces it,
/// last-wins), exactly matching how the engine's invalidation compares
/// methods via `method_signature_equivalent`. Method-table insertion order
/// and global indices are deliberately excluded so the key is identical
/// across re-compilations of the same source.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct MethodKey {
    identity: ReplMethodIdentity,
}

impl MethodKey {
    /// Key for a method-table entry: the table name plus the entry's
    /// canonical `core_signature`.
    pub(crate) fn from_method_sig(function: &str, sig: &MethodSig) -> Self {
        Self {
            identity: ReplMethodIdentity::from_method_sig(function, sig),
        }
    }

    /// Key for a function-table definition, using the definition's own name.
    pub(crate) fn from_function(func: &Function) -> Self {
        Self::for_named_function(&func.name, func)
    }

    /// Key for a function-table definition registered under `function` (which
    /// can differ from `func.name`, e.g. a module-qualified table key). The
    /// canonical signature is derived exactly like
    /// `MethodSig::compute_core_signature_from_julia_projections` so
    /// method-table and function-table identities for the same definition
    /// coincide.
    pub(crate) fn for_named_function(function: &str, func: &Function) -> Self {
        Self {
            identity: ReplMethodIdentity::from_function(function, func),
        }
    }

    /// Test-only constructor from an explicit canonical signature.
    #[cfg(test)]
    pub(crate) fn new(function: &str, signature: CoreType) -> Self {
        Self {
            identity: ReplMethodIdentity::from_parts(function.to_string(), signature, None, None),
        }
    }

    /// The generic-function name this method belongs to.
    pub(crate) fn function(&self) -> &str {
        self.identity.function()
    }

    /// Whether this key denotes the same method as `sig` (same canonical
    /// declared signature and vararg shape). Used to validate memoized callee
    /// keys without re-cloning the signature tree — `global_index` alone is
    /// not a reliable identity (test tables and rebuilt tables can reuse it).
    pub(crate) fn matches_method_sig(&self, sig: &MethodSig) -> bool {
        self.identity.matches(self.function(), sig)
    }

    fn display(&self) -> String {
        format!(
            "{} :: {}",
            self.function(),
            self.identity.core_signature().to_julia_name()
        )
    }
}

/// Stable specialization identity (≈ upstream `MethodInstance`): a method
/// plus the canonical specialized argument signature (`specTypes`).
///
/// Built once per inference body entry on `using Optim`-class workloads
/// (~10⁵ entries, #8185), so the layout is tuned for cheap map operations:
///
/// - the method half is shared via [`Rc`] and memoized per method identity by
///   the engine, so only the specialized argument tuple is converted per
///   entry;
/// - a structural hash is precomputed at construction ([`Self::hash`] writes
///   just that `u64`), so `HashMap` operations never re-walk the `CoreType`
///   trees;
/// - equality compares the cached hash first, and interned keys additionally
///   short-circuit through `Rc`'s pointer-equality fast path.
#[derive(Clone, Debug, Eq)]
pub(crate) struct SpecializationKey {
    /// Precomputed structural hash of `(method, spec_types)`. First field so
    /// derived equality checks it before walking the type trees.
    cached_hash: u64,
    method: Rc<MethodKey>,
    /// Canonical `Tuple{...}` of the call-site argument types after the
    /// shared cache-key widening policy (const-eligible values stay as
    /// `CoreType::Value` params, everything else widens).
    spec_types: CoreType,
}

impl PartialEq for SpecializationKey {
    fn eq(&self, other: &Self) -> bool {
        self.cached_hash == other.cached_hash
            && self.method == other.method
            && self.spec_types == other.spec_types
    }
}

impl std::hash::Hash for SpecializationKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        state.write_u64(self.cached_hash);
    }
}

impl SpecializationKey {
    /// Test-only convenience over [`Self::from_shared_method`]; production
    /// paths intern the [`MethodKey`] first.
    #[cfg(test)]
    pub(crate) fn new(method: MethodKey, argtypes: &[CacheArgType]) -> Self {
        Self::from_shared_method(Rc::new(method), argtypes)
    }

    /// Builds a specialization from an already-interned method identity,
    /// avoiding the per-entry canonical signature rebuild (Issue #8553 /
    /// #8185 budget).
    pub(crate) fn from_shared_method(method: Rc<MethodKey>, argtypes: &[CacheArgType]) -> Self {
        let spec_types = cache_argtypes_to_spec_tuple(argtypes);
        let cached_hash = {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            method.hash(&mut hasher);
            spec_types.hash(&mut hasher);
            hasher.finish()
        };
        Self {
            cached_hash,
            method,
            spec_types,
        }
    }

    fn display(&self) -> String {
        format!(
            "{} @ {}",
            self.method.display(),
            self.spec_types.to_julia_name()
        )
    }
}

/// The generic-function base name used for alias-tolerant caller lookups
/// during the invalidation walk (Issue #8554).
///
/// A module-qualified name (`MyMod.helper`) and its bare form (`helper`) can
/// denote aliased identities across the recording sites (function-table keys
/// vs `Function::name`-derived method keys), so the reverse walk unifies them
/// by the segment after the last `.` and errs toward over-invalidation.
fn function_base_name(name: &str) -> &str {
    match name.rfind('.') {
        Some(idx) => &name[idx + 1..],
        None => name,
    }
}

/// Widens `ty` into the trusted core fragment on which
/// [`CoreType::type_intersect`]'s `Bottom` verdict is a *proof* of
/// disjointness, or returns `None` when `ty` leaves that fragment.
///
/// `type_intersect` is conservative in the opposite direction from what
/// invalidation needs: its catch-all arm answers `Bottom` ("disjoint") for
/// shape pairs the hierarchy-less lattice does not model (user nominal types
/// and their declared supertypes, value params vs carrier types, `where`
/// signatures, vararg shapes). Trusting such a verdict would
/// *under*-invalidate — the cardinal sin for cache invalidation (Issue #5966:
/// stale-dispatch bugs are dispatch-order/cache dependent and surface only in
/// full-suite runs). The invalidation walk therefore only trusts a
/// disjointness verdict between types built entirely from builtin
/// primitives/abstracts, tuples, unions, and `Type{...}` wrappers thereof;
/// const value params widen to their carrier type first (sound:
/// `Value(v) <: widen(v)`, so any overlap of the value is an overlap of the
/// carrier). Everything else answers "may overlap" and over-invalidates.
pub(crate) fn widen_core_type_for_overlap(ty: &CoreType) -> Option<CoreType> {
    match ty {
        CoreType::Bottom | CoreType::Any | CoreType::Primitive(_) | CoreType::Abstract(_) => {
            Some(ty.clone())
        }
        CoreType::Value(value) => widen_value_param_for_overlap(value),
        CoreType::Tuple(elements) => elements
            .iter()
            .map(widen_core_type_for_overlap)
            .collect::<Option<Vec<_>>>()
            .map(CoreType::Tuple),
        CoreType::Union(variants) => variants
            .iter()
            .map(widen_core_type_for_overlap)
            .collect::<Option<Vec<_>>>()
            .map(CoreType::Union),
        CoreType::TypeOf(inner) => {
            widen_core_type_for_overlap(inner).map(|inner| CoreType::TypeOf(Box::new(inner)))
        }
        // User nominal types (their hierarchy is not visible here), type
        // variables, `where` wrappers, vararg shapes, named tuples, modules,
        // and opaque names: outside the trusted fragment.
        CoreType::AbstractUser { .. }
        | CoreType::Struct { .. }
        | CoreType::Vararg(_)
        | CoreType::VarargLen { .. }
        | CoreType::NamedTuple(_)
        | CoreType::TypeVar(_)
        | CoreType::UnionAll { .. }
        | CoreType::Module(_)
        | CoreType::Named(_) => None,
    }
}

fn widen_value_param_for_overlap(value: &CoreValueParam) -> Option<CoreType> {
    match value {
        CoreValueParam::Int(_) => Some(CoreType::Primitive(CorePrimitive::Int64)),
        CoreValueParam::Bool(_) => Some(CoreType::Primitive(CorePrimitive::Bool)),
        CoreValueParam::Symbol(_) => Some(CoreType::Primitive(CorePrimitive::Symbol)),
        CoreValueParam::String(_) => Some(CoreType::Primitive(CorePrimitive::String)),
        CoreValueParam::SignedInt { bits, .. } => Some(CoreType::Primitive(match bits {
            8 => CorePrimitive::Int8,
            16 => CorePrimitive::Int16,
            32 => CorePrimitive::Int32,
            64 => CorePrimitive::Int64,
            128 => CorePrimitive::Int128,
            _ => return None,
        })),
        CoreValueParam::UnsignedInt { bits, .. } => Some(CoreType::Primitive(match bits {
            8 => CorePrimitive::UInt8,
            16 => CorePrimitive::UInt16,
            32 => CorePrimitive::UInt32,
            64 => CorePrimitive::UInt64,
            128 => CorePrimitive::UInt128,
            _ => return None,
        })),
    }
}

/// Over-approximate overlap test between two canonical argtype/spec tuples
/// (Issue #8554): `false` only when the trusted fragment *proves* the two
/// signatures disjoint; any uncertainty answers `true` (over-invalidate).
pub(crate) fn spec_tuples_may_overlap(left: &CoreType, right: &CoreType) -> bool {
    let (Some(left), Some(right)) = (
        widen_core_type_for_overlap(left),
        widen_core_type_for_overlap(right),
    ) else {
        return true;
    };
    !matches!(left.type_intersect(&right), CoreType::Bottom)
}

/// Whether caller edge `edge` may consume the (re-)inference result of
/// specialization `spec` — the transitive step of the invalidation walk
/// (Issue #8554).
///
/// A resolved edge to the *same* generic function but a *different* method
/// identity provably does not depend on `spec` (its result came from another
/// method), so it is skipped; every other name-aliased edge falls back to the
/// conservative argtype-overlap test.
fn edge_may_reach_specialization(edge: &CallEdge, spec: &SpecializationKey) -> bool {
    let spec_fn = spec.method.function();
    let edge_fn = edge.callee.function();
    if function_base_name(edge_fn) != function_base_name(spec_fn) {
        return false;
    }
    if let BackedgeCallee::Method(callee) = &edge.callee {
        if edge_fn == spec_fn && callee.as_ref() != spec.method.as_ref() {
            // Same generic function, resolved to a different method: this
            // caller does not consume `spec`'s result.
            return false;
        }
    }
    spec_tuples_may_overlap(&edge.call_argtypes, &spec.spec_types)
}

/// Which call form produced a recorded edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum CallEdgeKind {
    /// A direct (bare-name) call resolved through the function/method tables.
    Direct,
    /// A module-qualified `Expr::ModuleCall` site.
    ModuleQualified,
    /// A dynamic-fallback site where a static target was attempted but did
    /// not resolve to a single method (imprecise argtypes or dispatch miss).
    DynamicFallback,
}

impl CallEdgeKind {
    fn display(self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::ModuleQualified => "module-qualified",
            Self::DynamicFallback => "dynamic-fallback",
        }
    }
}

/// The callee side of a recorded call edge.
///
/// Resolved method identities are shared via [`Rc`]: the engine memoizes one
/// [`MethodKey`] per callee method, so building an edge never re-clones the
/// canonical signature tree and edge comparisons hit `Rc`'s pointer-equality
/// fast path (#8185 budget).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum BackedgeCallee {
    /// Resolved to a specific method.
    Method(Rc<MethodKey>),
    /// A static target was attempted under this generic-function name but no
    /// single method resolved; a later method (re)definition under the name
    /// may capture the call.
    Unresolved { function: String },
}

impl BackedgeCallee {
    /// The generic-function name used by the reverse (invalidation-walk)
    /// index.
    fn function(&self) -> &str {
        match self {
            Self::Method(key) => key.function(),
            Self::Unresolved { function } => function,
        }
    }

    fn display(&self) -> String {
        match self {
            Self::Method(key) => key.display(),
            Self::Unresolved { function } => format!("{function} (unresolved)"),
        }
    }
}

/// One recorded `caller → callee` inference edge.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct CallEdge {
    pub(crate) callee: BackedgeCallee,
    /// Canonical `Tuple{...}` of the observed call-site argument types.
    pub(crate) call_argtypes: CoreType,
    pub(crate) kind: CallEdgeKind,
}

/// The recorded precise backedge graph (record-only in this slice).
///
/// Recording runs at every resolved call edge during inference, so the maps
/// are tuned for cheap operations: `HashMap`s keyed by interned
/// [`Rc<SpecializationKey>`]s (whose hash is precomputed and whose equality
/// short-circuits through `Rc` pointer identity for interned keys) and small
/// `Vec` edge/caller sets deduplicated by membership check. [`Self::dump`]
/// sorts its output lines, so determinism does not depend on map order.
#[derive(Debug, Default)]
pub(crate) struct BackedgeIndex {
    /// Forward: caller specialization → recorded call edges (deduplicated).
    call_edges: HashMap<Rc<SpecializationKey>, Vec<CallEdge>>,
    /// Forward: caller specialization → global binding names read.
    global_edges: HashMap<Rc<SpecializationKey>, BTreeSet<String>>,
    /// Reverse: callee generic-function name → caller specializations. This
    /// is what the #8554 invalidation walk starts from on a method mutation.
    callers_by_callee: HashMap<String, Vec<Rc<SpecializationKey>>>,
    /// Reverse: binding name → reader specializations (binding analogue).
    callers_by_binding: HashMap<String, Vec<Rc<SpecializationKey>>>,
    /// Per-specialization validity window (upstream `CodeInstance`
    /// `min_world`/`max_world` analogue), doubling as the intern table for
    /// caller keys. Stamped open-ended at the world the specialization is
    /// first seen; #8554 caps `max_world` on invalidation.
    worlds: HashMap<Rc<SpecializationKey>, WorldRange>,
}

impl BackedgeIndex {
    /// Interns `caller`, reusing the existing shared allocation when the
    /// specialization is already known, and stamps its world range. The
    /// engine interns each caller once per body entry so subsequent map
    /// operations hit the `Rc` pointer-equality fast path.
    pub(crate) fn intern_caller(
        &mut self,
        caller: &Rc<SpecializationKey>,
        world: World,
    ) -> Rc<SpecializationKey> {
        if let Some((existing, _)) = self.worlds.get_key_value(caller.as_ref()) {
            return Rc::clone(existing);
        }
        self.worlds
            .insert(Rc::clone(caller), WorldRange::from_world(world));
        Rc::clone(caller)
    }

    /// Records one `caller → callee` call edge observed at `world`.
    pub(crate) fn record_call_edge(
        &mut self,
        caller: &Rc<SpecializationKey>,
        edge: CallEdge,
        world: World,
    ) {
        let caller = self.intern_caller(caller, world);
        // `get_mut` first: the callee name is almost always already present,
        // and this avoids allocating the lookup `String` per record.
        if let Some(callers) = self.callers_by_callee.get_mut(edge.callee.function()) {
            if !callers.iter().any(|known| Rc::ptr_eq(known, &caller)) {
                callers.push(Rc::clone(&caller));
            }
        } else {
            self.callers_by_callee
                .insert(edge.callee.function().to_string(), vec![Rc::clone(&caller)]);
        }
        let edges = self.call_edges.entry(caller).or_default();
        if !edges.contains(&edge) {
            edges.push(edge);
        }
    }

    /// Records one `caller → global binding` read edge observed at `world`.
    pub(crate) fn record_global_read(
        &mut self,
        caller: &Rc<SpecializationKey>,
        binding: &str,
        world: World,
    ) {
        let caller = self.intern_caller(caller, world);
        if let Some(readers) = self.callers_by_binding.get_mut(binding) {
            if !readers.iter().any(|known| Rc::ptr_eq(known, &caller)) {
                readers.push(Rc::clone(&caller));
            }
        } else {
            self.callers_by_binding
                .insert(binding.to_string(), vec![Rc::clone(&caller)]);
        }
        self.global_edges
            .entry(caller)
            .or_default()
            .insert(binding.to_string());
    }

    /// Caller specializations with a recorded call edge to exactly
    /// `mutated_fn` for which `edge_affected` accepts at least one such edge —
    /// the direct seeds of a method-mutation invalidation walk (Issue #8554,
    /// upstream analogue: the signature-intersection scan in
    /// `julia/Compiler/src/reinfer.jl` / `jl_method_table_insert`).
    ///
    /// Seeding matches the callee name exactly: edges are recorded under the
    /// same table names mutations use. (The *transitive* step is
    /// alias-tolerant, see [`Self::transitively_affected`].)
    pub(crate) fn method_mutation_seeds(
        &self,
        mutated_fn: &str,
        mut edge_affected: impl FnMut(&CallEdge) -> bool,
    ) -> Vec<Rc<SpecializationKey>> {
        let Some(callers) = self.callers_by_callee.get(mutated_fn) else {
            return Vec::new();
        };
        let mut seeds = Vec::new();
        for caller in callers {
            let Some(edges) = self.call_edges.get(caller.as_ref()) else {
                continue;
            };
            if edges
                .iter()
                .any(|edge| edge.callee.function() == mutated_fn && edge_affected(edge))
            {
                seeds.push(Rc::clone(caller));
            }
        }
        seeds
    }

    /// Reader specializations of any binding in `changed` — the direct seeds
    /// of a binding-mutation invalidation walk (Issue #8554, upstream
    /// analogue: `should_invalidate_code_for_globalref` in
    /// `julia/Compiler/src/bindinginvalidations.jl`).
    pub(crate) fn binding_mutation_seeds(
        &self,
        changed: &BTreeSet<String>,
    ) -> Vec<Rc<SpecializationKey>> {
        let mut seeds: Vec<Rc<SpecializationKey>> = Vec::new();
        for binding in changed {
            let Some(readers) = self.callers_by_binding.get(binding) else {
                continue;
            };
            for reader in readers {
                if !seeds.iter().any(|known| Rc::ptr_eq(known, reader)) {
                    seeds.push(Rc::clone(reader));
                }
            }
        }
        seeds
    }

    /// Transitive closure of `seeds` over the reverse call graph: every
    /// specialization whose recorded result may (transitively) consume the
    /// result of an affected specialization (Issue #8554).
    ///
    /// Iterative worklist, cycle-safe (the `affected` set doubles as the
    /// visited set, so mutually-recursive specializations are processed
    /// once). Caller lookup is alias-tolerant on the generic-function base
    /// name (see [`function_base_name`]); per-edge reachability is decided by
    /// [`edge_may_reach_specialization`], which over-approximates outside the
    /// trusted core fragment.
    pub(crate) fn transitively_affected(
        &self,
        seeds: Vec<Rc<SpecializationKey>>,
    ) -> HashSet<Rc<SpecializationKey>> {
        // One pass over the reverse-index keys builds the alias buckets, so
        // the walk itself stays O(edges reached).
        let mut buckets_by_base: HashMap<&str, Vec<&str>> = HashMap::new();
        for name in self.callers_by_callee.keys() {
            buckets_by_base
                .entry(function_base_name(name))
                .or_default()
                .push(name.as_str());
        }

        let mut affected: HashSet<Rc<SpecializationKey>> = HashSet::new();
        let mut worklist = seeds;
        while let Some(spec) = worklist.pop() {
            if !affected.insert(Rc::clone(&spec)) {
                continue;
            }
            let callee_base = function_base_name(spec.method.function());
            let Some(buckets) = buckets_by_base.get(callee_base) else {
                continue;
            };
            for bucket in buckets {
                let Some(callers) = self.callers_by_callee.get(*bucket) else {
                    continue;
                };
                for caller in callers {
                    if affected.contains(caller.as_ref()) {
                        continue;
                    }
                    let Some(edges) = self.call_edges.get(caller.as_ref()) else {
                        continue;
                    };
                    if edges
                        .iter()
                        .any(|edge| edge_may_reach_specialization(edge, &spec))
                    {
                        worklist.push(Rc::clone(caller));
                    }
                }
            }
        }
        affected
    }

    /// Caps the validity window of every specialization in `affected` so it
    /// no longer includes `bound` (Issue #8554; upstream: capping
    /// `CodeInstance.max_world` during the backedge walk). Recorded edges are
    /// intentionally kept: a re-inference re-records into the same interned
    /// specialization, and a stale leftover edge can only over-invalidate,
    /// never under-invalidate.
    pub(crate) fn cap_specializations_before(
        &mut self,
        affected: &HashSet<Rc<SpecializationKey>>,
        bound: World,
    ) {
        for spec in affected {
            if let Some(range) = self.worlds.get_mut(spec.as_ref()) {
                range.cap_before(bound);
            }
        }
    }

    /// Call edges recorded for `caller`, if any. Test introspection for the
    /// #8553/#8554 record/invalidate contract.
    #[cfg(test)]
    pub(crate) fn call_edges_for(&self, caller: &SpecializationKey) -> Option<&[CallEdge]> {
        self.call_edges.get(caller).map(Vec::as_slice)
    }

    /// Global binding reads recorded for `caller`, if any. Test introspection.
    #[cfg(test)]
    pub(crate) fn global_reads_for(&self, caller: &SpecializationKey) -> Option<&BTreeSet<String>> {
        self.global_edges.get(caller)
    }

    /// Caller specializations that recorded an edge to generic-function
    /// `callee` (resolved or attempted). Empty when none. Test introspection;
    /// the production walk starts from [`Self::method_mutation_seeds`].
    #[cfg(test)]
    pub(crate) fn caller_specializations_of(&self, callee: &str) -> Vec<Rc<SpecializationKey>> {
        self.callers_by_callee
            .get(callee)
            .cloned()
            .unwrap_or_default()
    }

    /// Reader specializations of top-level `binding`. Empty when none.
    /// Test introspection; the production walk starts from
    /// [`Self::binding_mutation_seeds`].
    #[cfg(test)]
    pub(crate) fn binding_reader_specializations_of(
        &self,
        binding: &str,
    ) -> Vec<Rc<SpecializationKey>> {
        self.callers_by_binding
            .get(binding)
            .cloned()
            .unwrap_or_default()
    }

    /// The validity window stamped for `caller`, if it was interned.
    /// Test introspection for the #8554 world-capping behavior.
    #[cfg(test)]
    pub(crate) fn specialization_world(&self, caller: &SpecializationKey) -> Option<WorldRange> {
        self.worlds.get(caller).copied()
    }

    /// Deterministic textual dump of the recorded graph, one edge per line
    /// (lines are sorted, so the output is stable across map iteration
    /// orders):
    ///
    /// ```text
    /// call caller :: Tuple{Int64} @ Tuple{Int64} -> [direct] callee :: Tuple{Int64} @ Tuple{Int64} [world 1..]
    /// global reader :: Tuple{} @ Tuple{} -> BINDING [world 1..]
    /// ```
    pub(crate) fn dump(&self) -> String {
        let mut lines = Vec::new();
        for (caller, edges) in &self.call_edges {
            let world = self.world_suffix(caller);
            for edge in edges {
                lines.push(format!(
                    "call {} -> [{}] {} @ {}{}",
                    caller.display(),
                    edge.kind.display(),
                    edge.callee.display(),
                    edge.call_argtypes.to_julia_name(),
                    world,
                ));
            }
        }
        for (caller, bindings) in &self.global_edges {
            let world = self.world_suffix(caller);
            for binding in bindings {
                lines.push(format!(
                    "global {} -> {}{}",
                    caller.display(),
                    binding,
                    world
                ));
            }
        }
        lines.sort_unstable();
        let mut out = String::new();
        for line in lines {
            out.push_str(&line);
            out.push('\n');
        }
        out
    }

    fn world_suffix(&self, caller: &SpecializationKey) -> String {
        match self.worlds.get(caller) {
            Some(range) if range.max_world == World::MAX => {
                format!(" [world {}..]", range.min_world)
            }
            Some(range) => format!(" [world {}..{}]", range.min_world, range.max_world),
            None => String::new(),
        }
    }
}

/// Canonical `Tuple{...}` spec signature from cache-key argument slots.
///
/// Const-eligible values become `CoreType::Value` params (so e.g. the
/// `Val{true}`-style `Bool` specializations stay distinct); widened slots map
/// through the existing lattice → Julia → core bridge.
pub(crate) fn cache_argtypes_to_spec_tuple(argtypes: &[CacheArgType]) -> CoreType {
    CoreType::Tuple(argtypes.iter().map(cache_argtype_to_core).collect())
}

/// Canonical `Tuple{...}` edge signature straight from lattice call argtypes.
///
/// Uses the direct lattice → core conversion (`Const` values widen to their
/// concrete type, mirroring upstream backedge signatures, which carry types
/// rather than constants). Deliberately NOT routed through the `JuliaType`
/// string bridge — this runs per recorded call edge (#8185 budget).
pub(crate) fn lattice_argtypes_to_spec_tuple(argtypes: &[LatticeType]) -> CoreType {
    CoreType::Tuple(argtypes.iter().map(CoreType::from).collect())
}

fn cache_argtype_to_core(arg: &CacheArgType) -> CoreType {
    match arg {
        CacheArgType::Const(cv) => match cv {
            ConstValue::Bool(b) => CoreType::Value(CoreValueParam::Bool(*b)),
            ConstValue::Int64(n) => CoreType::Value(CoreValueParam::Int(*n)),
            ConstValue::Symbol(s) => CoreType::Value(CoreValueParam::Symbol(s.clone())),
            ConstValue::Nothing => CoreType::Primitive(CorePrimitive::Nothing),
            // Not produced by the widening policy, but keep the mapping total.
            ConstValue::Float64(_) | ConstValue::String(_) => {
                widened_lattice_to_core(&LatticeType::Concrete(cv.to_concrete_type()))
            }
        },
        CacheArgType::Type(lattice) => widened_lattice_to_core(lattice),
    }
}

fn widened_lattice_to_core(lattice: &LatticeType) -> CoreType {
    // Direct lattice → core conversion. Deliberately NOT routed through the
    // `lattice_to_julia_type` string bridge: this runs on every body entry
    // during inference, and the bridge's name formatting/re-parsing was the
    // dominant recording cost on `using Optim`-class workloads (#8185).
    CoreType::from(lattice)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::lattice::types::ConcreteType;
    use crate::inference_core::CorePrimitive;

    fn int_spec(function: &str) -> Rc<SpecializationKey> {
        Rc::new(SpecializationKey::new(
            MethodKey::new(
                function,
                CoreType::Tuple(vec![CoreType::Primitive(CorePrimitive::Int64)]),
            ),
            &[CacheArgType::Type(LatticeType::Concrete(
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
            ))],
        ))
    }

    #[test]
    fn issue_8553_const_slots_keep_value_identity_in_spec_tuple() {
        let true_tuple =
            cache_argtypes_to_spec_tuple(&[CacheArgType::Const(ConstValue::Bool(true))]);
        let false_tuple =
            cache_argtypes_to_spec_tuple(&[CacheArgType::Const(ConstValue::Bool(false))]);
        assert_ne!(
            true_tuple, false_tuple,
            "const-preserved slots must stay distinct in the spec signature"
        );

        let widened = cache_argtypes_to_spec_tuple(&[CacheArgType::Type(LatticeType::Concrete(
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool)),
        ))]);
        assert_eq!(
            widened,
            CoreType::Tuple(vec![CoreType::Primitive(CorePrimitive::Bool)])
        );
    }

    #[test]
    fn issue_8553_record_call_edge_dedups_and_stamps_world_once() {
        let mut index = BackedgeIndex::default();
        let caller = int_spec("caller");
        let edge = CallEdge {
            callee: BackedgeCallee::Unresolved {
                function: "callee".to_string(),
            },
            call_argtypes: CoreType::Tuple(vec![CoreType::Primitive(CorePrimitive::Int64)]),
            kind: CallEdgeKind::DynamicFallback,
        };

        index.record_call_edge(&caller, edge.clone(), 3);
        // Same edge at a later world: deduped, and the original stamp wins.
        index.record_call_edge(&caller, edge, 7);

        assert_eq!(
            index.call_edges_for(&caller).map(<[CallEdge]>::len),
            Some(1),
            "identical edges must be recorded once"
        );
        assert_eq!(
            index.specialization_world(&caller),
            Some(WorldRange::from_world(3)),
            "the first-recorded world stamp is retained"
        );
        assert_eq!(index.caller_specializations_of("callee").len(), 1);
        assert!(index.caller_specializations_of("other").is_empty());
    }

    #[test]
    fn issue_8553_global_read_edges_are_indexed_both_ways() {
        let mut index = BackedgeIndex::default();
        let caller = int_spec("reader");
        index.record_global_read(&caller, "G", 1);

        assert!(index
            .global_reads_for(&caller)
            .is_some_and(|reads| reads.contains("G")));
        assert_eq!(index.binding_reader_specializations_of("G").len(), 1);
        assert!(index.binding_reader_specializations_of("H").is_empty());
        assert!(index.dump().contains("global"));
    }

    fn method_edge_to(spec: &SpecializationKey, argtypes: CoreType) -> CallEdge {
        CallEdge {
            callee: BackedgeCallee::Method(Rc::clone(&spec.method)),
            call_argtypes: argtypes,
            kind: CallEdgeKind::Direct,
        }
    }

    #[test]
    fn issue_8554_overlap_widens_value_params_and_distrusts_user_nominals() {
        // Value params widen to their carrier type: Value(5) overlaps Int64...
        assert!(spec_tuples_may_overlap(
            &CoreType::Tuple(vec![CoreType::Value(CoreValueParam::Int(5))]),
            &CoreType::Tuple(vec![CoreType::Primitive(CorePrimitive::Int64)]),
        ));
        // ...but is provably disjoint from String.
        assert!(!spec_tuples_may_overlap(
            &CoreType::Tuple(vec![CoreType::Value(CoreValueParam::Int(5))]),
            &CoreType::Tuple(vec![CoreType::Primitive(CorePrimitive::String)]),
        ));
        // Trusted disjointness: Int64 vs Float64.
        assert!(!spec_tuples_may_overlap(
            &CoreType::Tuple(vec![CoreType::Primitive(CorePrimitive::Int64)]),
            &CoreType::Tuple(vec![CoreType::Primitive(CorePrimitive::Float64)]),
        ));
        // User nominal types leave the trusted fragment: never claim disjoint,
        // even though the hierarchy-less intersection would answer Bottom.
        assert!(spec_tuples_may_overlap(
            &CoreType::Tuple(vec![CoreType::Named("MyAbstract".to_string())]),
            &CoreType::Tuple(vec![CoreType::Primitive(CorePrimitive::Int64)]),
        ));
        // `where`-parametric signatures are likewise conservative.
        assert!(spec_tuples_may_overlap(
            &CoreType::UnionAll {
                var: CoreTypeVar::unscoped("T"),
                body: Box::new(CoreType::Tuple(vec![CoreType::TypeVar(
                    CoreTypeVar::unscoped("T"),
                )])),
            },
            &CoreType::Tuple(vec![CoreType::Primitive(CorePrimitive::Int64)]),
        ));
    }

    #[test]
    fn issue_8554_transitive_walk_is_cycle_safe_and_reaches_callers() {
        let mut index = BackedgeIndex::default();
        let a = int_spec("walk_a");
        let b = int_spec("walk_b");
        // Mutual recursion: a calls b, b calls a.
        index.record_call_edge(&a, method_edge_to(&b, int_core_tuple()), 1);
        index.record_call_edge(&b, method_edge_to(&a, int_core_tuple()), 1);

        let affected = index.transitively_affected(vec![Rc::clone(&a)]);
        assert!(affected.contains(a.as_ref()), "seed must be affected");
        assert!(
            affected.contains(b.as_ref()),
            "the caller of an affected specialization must be reached \
             (cycle-safe worklist)"
        );
        assert_eq!(affected.len(), 2);
    }

    #[test]
    fn issue_8554_transitive_walk_skips_argtype_disjoint_callers() {
        let mut index = BackedgeIndex::default();
        let callee = int_spec("walk_callee");
        let int_caller = int_spec("walk_int_caller");
        let float_caller = int_spec("walk_float_caller");
        index.record_call_edge(&int_caller, method_edge_to(&callee, int_core_tuple()), 1);
        index.record_call_edge(
            &float_caller,
            method_edge_to(
                &callee,
                CoreType::Tuple(vec![CoreType::Primitive(CorePrimitive::Float64)]),
            ),
            1,
        );

        // `callee`'s specialization is Tuple{Int64}: the Float64 call site
        // provably does not consume it.
        let affected = index.transitively_affected(vec![Rc::clone(&callee)]);
        assert!(affected.contains(int_caller.as_ref()));
        assert!(
            !affected.contains(float_caller.as_ref()),
            "an argtype-disjoint caller must survive the transitive walk"
        );
    }

    #[test]
    fn issue_8554_transitive_walk_is_alias_tolerant_on_base_names() {
        let mut index = BackedgeIndex::default();
        // The callee body was entered under its bare name...
        let callee = int_spec("helper");
        // ...but the caller recorded a module-qualified edge.
        let caller = int_spec("alias_caller");
        let qualified = Rc::new(MethodKey::new(
            "MyMod.helper",
            CoreType::Tuple(vec![CoreType::Primitive(CorePrimitive::Int64)]),
        ));
        index.record_call_edge(
            &caller,
            CallEdge {
                callee: BackedgeCallee::Method(qualified),
                call_argtypes: int_core_tuple(),
                kind: CallEdgeKind::ModuleQualified,
            },
            1,
        );

        let affected = index.transitively_affected(vec![Rc::clone(&callee)]);
        assert!(
            affected.contains(caller.as_ref()),
            "base-name aliasing must err toward over-invalidation"
        );
    }

    fn int_core_tuple() -> CoreType {
        CoreType::Tuple(vec![CoreType::Primitive(CorePrimitive::Int64)])
    }

    #[test]
    fn issue_8554_cap_specializations_before_retires_affected_worlds() {
        let mut index = BackedgeIndex::default();
        let affected_spec = int_spec("capped");
        let survivor_spec = int_spec("survivor");
        index.record_global_read(&affected_spec, "G", 3);
        index.record_global_read(&survivor_spec, "H", 3);

        let mut affected = HashSet::new();
        affected.insert(Rc::clone(&affected_spec));
        index.cap_specializations_before(&affected, 4);

        assert_eq!(
            index
                .specialization_world(&affected_spec)
                .map(|range| range.max_world),
            Some(3),
            "affected specializations must have max_world capped"
        );
        assert_eq!(
            index.specialization_world(&survivor_spec),
            Some(WorldRange::from_world(3)),
            "unaffected specializations keep their open-ended window"
        );
    }
}
