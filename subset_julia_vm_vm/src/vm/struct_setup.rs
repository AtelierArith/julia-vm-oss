//! VM start-up struct/abstract-type table setup (Issue #6334: extracted
//! from `vm/mod.rs`): struct hierarchy construction, the precomputed
//! abstract-ancestor closure, and reflection struct-def normalization.

use super::{AbstractTypeDefInfo, CompiledProgram, StructDefInfo};
use crate::types::{nominal_family_name, StructHierarchy};
use std::collections::{HashMap, HashSet};

/// Rename the reflection `Method` struct's `mod` field to `module` (Issue #5125).
///
/// Upstream `Method` exposes `.module::Module`, but `module` is a reserved
/// keyword that the parser cannot accept as a struct field name, so the
/// pure-Julia `struct Method` declares the field as `mod`. This rewrites that
/// field name in the loaded struct-definition table to `module` so that
/// `m.module` field access (compiled to `GetFieldByName("module")`) resolves and
/// `fieldnames(Method)` reports `:module`. The placeholder `mod` name is never
/// user-visible.
pub(super) fn normalize_method_struct_def(struct_defs: &mut [StructDefInfo]) {
    if let Some(def) = struct_defs.iter_mut().find(|def| def.name == "Method") {
        if let Some((name, _)) = def.fields.iter_mut().find(|(name, _)| name == "mod") {
            *name = "module".to_string();
        }
    }
}

pub(super) fn build_struct_hierarchy_from_program(program: &CompiledProgram) -> StructHierarchy {
    let mut hierarchy = StructHierarchy::new();

    // Register parametric struct templates FIRST so the family key (e.g. `SVector`)
    // keeps the template's declared parent AND its type-parameter NAMES
    // (`["N", "T"]`). A parametric struct's monomorphized instances
    // (`SVector{Any, Any}`, `SVector{3, Int64}`) collapse to the same family key
    // via `nominal_family_name`; if those concrete `struct_defs` were inserted
    // first they clobbered the entry with an EMPTY type-parameter list, so
    // `registered_instantiated_struct_parent_in` could not substitute the concrete
    // arguments into the parent template and the parametric parent edge through a
    // value-parameter intermediate (`StaticVector{N,T} <: StaticVecOrMat{Tuple{N},
    // T,1}`) was lost — `SVector{3,Int64} isa AbstractArray{Int64,1}` returned
    // false (Issue #7728 / #7819).
    if let Some(ctx) = &program.compile_context {
        for (name, ps) in &ctx.parametric_structs {
            let type_params = ps
                .def
                .type_params
                .iter()
                .map(|param| param.name.clone())
                .collect();
            hierarchy.insert_if_absent(name, ps.def.parent_type.clone(), type_params);
        }
    }

    // Concrete (non-parametric) structs register their own family entry, but a
    // parametric struct's monomorphized instance must NOT overwrite the template
    // entry registered above, so use `insert_if_absent` here too.
    for def in &program.struct_defs {
        hierarchy.insert_if_absent(&def.name, def.parent_type.clone(), Vec::new());
    }

    // User-declared primitive types are nominal leaves too. Reflection already
    // stores their declared parent for `supertype(T)`; `<:` must see the same
    // edge through the shared hierarchy used by the Core subtype engine.
    for def in &program.primitive_types {
        hierarchy.insert_if_absent(&def.name, def.parent.clone(), Vec::new());
    }

    // `abstract type` declarations otherwise keep first-declaration priority
    // (matching the `struct_defs`/`parametric_structs` passes above) so a
    // duplicate re-scan of the same abstract type can't clobber its
    // correctly-linked parent. But `StructHierarchy` has no module scoping: a
    // package's own `abstract type Set end` (e.g. AbstractAlgebra, which
    // explicitly exports its own `Set` distinct from `Base.Set`) shares the
    // same bare-name slot as Base's concrete `struct Set{T} <:
    // AbstractSet{T}`, registered above via `struct_defs`. Under first-wins
    // semantics that struct entry silently blocks the abstract type's own
    // (unrelated) parent link, so `Integers{BigInt} <: Ring <: NCRing <: Set`
    // incorrectly resolves through Base's `AbstractSet`, and `println` picks
    // `Base.show(io, ::AbstractSet)` over the package's own `show` —
    // crashing when it tries to `iterate` a non-iterable value (Issue
    // #8861). An abstract type declaration always names a genuinely
    // different type than any same-named struct/parametric struct, so it
    // must win over that cross-kind collision; first-wins is preserved only
    // among repeated abstract-type declarations of the same name.
    let mut seen_abstract_type_names = HashSet::new();
    for at in &program.abstract_types {
        if seen_abstract_type_names.insert(nominal_family_name(&at.name).to_string()) {
            hierarchy.insert(
                &at.name,
                at.parent.clone(),
                at.type_params.iter().map(|tp| tp.name.clone()).collect(),
            );
        }
    }

    hierarchy
}

/// Pre-compute the transitive closure of the abstract type hierarchy (Issue #3356).
///
/// For each struct and abstract type, walks the parent chain and collects all
/// ancestor type names. This makes `check_isa_with_abstract_resolved` (the
/// `isa` fast path) O(1). Runtime `<:` no longer reads this map — it goes
/// through the `CoreSubtypeEngine` over `struct_hierarchy` (Issue #5915 wave 3).
///
/// `parametric_struct_names` carries the registered base names for *parametric*
/// user structs (e.g. `"Box"`); declared parents come from `struct_hierarchy`.
/// Unlike concrete structs, parametric structs are not registered in
/// `struct_defs` (they are instantiated lazily per concrete parameter), so
/// without this their declared supertype was invisible to the `isa` ancestry.
/// That made `x isa AbstractBox` (and the bare `Box <: AbstractBox`)
/// spuriously false even though every instantiation is a subtype of the bare
/// abstract supertype in Julia (Issue #5052). Registering the chain under the
/// parametric base name lets the `isa` ancestry's base-name fallback resolve
/// it, while parameter *invariance* is unaffected: the chain stores the declared
/// parent verbatim (`AbstractBox{T}`), so it never matches a
/// differently-parameterised supertype like `AbstractBox{Number}`.
fn ancestor_base_name(s: &str) -> &str {
    s.find('{').map(|idx| &s[..idx]).unwrap_or(s)
}

/// Walk `start_parent` up the abstract-type parent chain, collecting every
/// ancestor type name (both the qualified `Parent{T}` form and its bare base
/// name). Shared by [`compute_type_ancestors`] (whole-program derivation) and
/// [`append_type_ancestors`] (incremental live-append, Issue #9199 LV4).
fn collect_ancestors(
    start_parent: &Option<String>,
    abstract_types: &[AbstractTypeDefInfo],
    abstract_type_name_index: &HashMap<String, usize>,
) -> Vec<String> {
    let mut chain = Vec::new();
    let mut current_parent = start_parent.clone();
    while let Some(ref parent) = current_parent {
        chain.push(parent.clone());
        let parent_base = ancestor_base_name(parent);
        if parent_base != parent.as_str() {
            chain.push(parent_base.to_string());
        }
        current_parent = abstract_type_name_index
            .get(parent_base)
            .and_then(|&idx| abstract_types.get(idx))
            .and_then(|at| at.parent.clone());
    }
    chain
}

pub(super) fn compute_type_ancestors(
    struct_defs: &[StructDefInfo],
    abstract_types: &[AbstractTypeDefInfo],
    abstract_type_name_index: &HashMap<String, usize>,
    struct_hierarchy: &StructHierarchy,
    parametric_struct_names: &[String],
) -> HashMap<String, Vec<String>> {
    let base_name = ancestor_base_name;

    let mut ancestors: HashMap<String, Vec<String>> = HashMap::new();

    for struct_def in struct_defs {
        let parent = struct_hierarchy.parent_for(&struct_def.name).flatten();
        let chain = collect_ancestors(&parent, abstract_types, abstract_type_name_index);
        if !chain.is_empty() {
            ancestors.insert(struct_def.name.clone(), chain);
        }
    }

    for abstract_type in abstract_types {
        let parent = struct_hierarchy.parent_for(&abstract_type.name).flatten();
        let chain = collect_ancestors(&parent, abstract_types, abstract_type_name_index);
        if !chain.is_empty() {
            ancestors.insert(abstract_type.name.clone(), chain);
        }
    }

    // Register parametric user structs under their base name (Issue #5052).
    // Their concrete instantiations (`Box{Int}`, ...) fall back to the base
    // name in the `isa` ancestry (`check_isa_with_abstract_resolved`), so the
    // declared supertype chain becomes reachable for `x::Box{Int} isa AbstractBox`.
    for name in parametric_struct_names {
        let base_name = base_name(name).to_string();
        let parent = struct_hierarchy.parent_for(name).flatten();
        let chain = collect_ancestors(&parent, abstract_types, abstract_type_name_index);
        if !chain.is_empty() {
            ancestors.entry(base_name).or_insert(chain);
        }
    }

    ancestors
}

/// Incrementally extend `type_ancestors` for a set of NEWLY live-appended
/// concrete struct names (Issue #9199 LV4).
///
/// A live type-append (`Vm::install_appended_types`) adds new leaf structs to
/// the live registries without a whole-program `Vm::new_program` rebuild. A new
/// leaf's ancestor chain is computed HERE using the ALREADY-EXTENDED live tables
/// (`struct_hierarchy` now holds the new struct's parent edge, `abstract_types` /
/// `abstract_type_name_index` are the live ones), then inserted. Existing entries
/// are never touched: a brand-new leaf type cannot become an ancestor of a
/// previously-defined type (its name did not exist when they were defined), so
/// the pre-existing ancestor chains stay correct — only the new names get
/// entries. `new_struct_names` are qualified `struct_defs` names.
pub(super) fn append_type_ancestors(
    type_ancestors: &mut HashMap<String, Vec<String>>,
    new_struct_names: &[String],
    abstract_types: &[AbstractTypeDefInfo],
    abstract_type_name_index: &HashMap<String, usize>,
    struct_hierarchy: &StructHierarchy,
) {
    for name in new_struct_names {
        let parent = struct_hierarchy.parent_for(name).flatten();
        let chain = collect_ancestors(&parent, abstract_types, abstract_type_name_index);
        if !chain.is_empty() {
            type_ancestors.insert(name.clone(), chain);
        }
    }
}
