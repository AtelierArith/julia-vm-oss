//! User-defined type alias expansion (Issue #5055).
//!
//! Supports both non-parametric aliases (`const IntVec = Vector{Int}`) and
//! parametric aliases (`MyVec{T} = Vector{T}`). Upstream Julia desugars a
//! parametric alias to a `UnionAll`-valued binding (`MyVec = (Vector{T} where
//! T)`), and a use `MyVec{Int}` instantiates that `UnionAll`, yielding
//! `Vector{Int}`. Since the no-JIT static pipeline resolves type names through
//! string-keyed lookups (`JuliaType::from_name_or_struct`), we model the same
//! behaviour by *expanding* an alias use into its target type string at
//! lowering time:
//!
//! ```text
//! MyVec{T} = Vector{T}   registers ("MyVec", params=["T"], target="Vector{T}")
//! MyVec{Int}             expands to "Vector{Int}" (positional substitution)
//! MyVec                  expands to "Vector"      (bare alias -> bare target base)
//! ```
//!
//! Non-parametric aliases register with an empty `params` list and expand by a
//! plain name swap. Expansion is applied at the three type-name chokepoints:
//! parametric type expressions, parametric-type call heads (constructors), and
//! function parameter annotations.
//!
//! The registry is thread-local and scoped to a single program/module lowering
//! pass (single-threaded per compilation in the static pipeline). It is
//! populated by a pre-scan before statement lowering and cleared afterwards.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::marker::PhantomData;
use std::rc::Rc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SourceIdentity(u64);

/// A byte position whose source-fragment identity is carried by construction.
/// Raw offsets from different parsed fragments are never comparable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourcePosition {
    source: SourceIdentity,
    byte_offset: usize,
}

impl SourcePosition {
    fn definition_is_visible_at(self, use_position: Self) -> bool {
        self.source != use_position.source || self.byte_offset <= use_position.byte_offset
    }
}

/// A registered alias: positional type parameter names and the target type
/// template string. For `MyVec{T} = Vector{T}` the params are `["T"]` and the
/// target template is `"Vector{T}"`. For `IntVec = Vector{Int}` the params are
/// empty and the target template is `"Vector{Int}"`.
#[derive(Debug, Clone)]
struct AliasEntry {
    owner: Vec<String>,
    params: Vec<String>,
    target: String,
    origin: Option<SourcePosition>,
    registration_order: u64,
    is_alias: bool,
    bare_fallback: bool,
}

thread_local! {
    static ALIASES: RefCell<HashMap<String, Vec<AliasEntry>>> = RefCell::new(HashMap::new());
    static NEXT_SOURCE_ID: Cell<u64> = const { Cell::new(1) };
    static CURRENT_SOURCE: Cell<Option<SourceIdentity>> = const { Cell::new(None) };
    static CURRENT_MODULE: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static NEXT_REGISTRATION_ORDER: Cell<u64> = const { Cell::new(1) };
    static RUNTIME_SIGNATURE_DEPTH: Cell<u32> = const { Cell::new(0) };
    /// Names bound at RUNTIME to a type value rather than registered as static
    /// string aliases — most notably a parametric alias with a `where`-clause
    /// RHS (`MyVec{T} = Vector{T} where T<:Real`), which lowers to a runtime
    /// `UnionAll`-valued binding (Issues #5053/#10372). A later bare-identifier
    /// assignment applying such a name (`z = MyVec{Float64}`) must lower as an
    /// ordinary runtime assignment instead of freezing the statically
    /// unexpandable application text as a new alias (Issue #10501).
    static RUNTIME_TYPE_BINDINGS: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
    /// Names DECLARED as types by the program currently being lowered — the
    /// `struct` / `mutable struct` / `abstract type` / `primitive type` heads
    /// found by the pre-scan (Issue #11104). A `const` / plain binding whose
    /// RHS is one of these names is a TYPE ALIAS (`const AE = E`), so it must
    /// register in the alias table and expand inside signature annotations;
    /// without this set the alias-detection gate only knew the builtin type
    /// names, so `f(x::AE)` registered a method on the nominal placeholder
    /// `AE` that no value is ever an instance of (`MethodError`).
    static DECLARED_TYPES: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
    /// `using`/`import` edges discovered by the pre-scan: (importing lexical
    /// scope, imported module path as spelled, leading dots stripped). A
    /// sibling module's alias may only resolve through the bare unique-leaf
    /// fallback when such an edge makes its owner visible (Issue #11452).
    static IMPORT_EDGES: RefCell<HashSet<(Vec<String>, Vec<String>)>> = RefCell::new(HashSet::new());
}

/// Scoped identity for one parsed source fragment. Byte offsets are comparable
/// only while this identity matches an alias entry's origin (Issue #11086).
pub struct SourceScope {
    previous: Option<SourceIdentity>,
    identity: SourceIdentity,
    _not_send: PhantomData<Rc<()>>,
}

impl SourceScope {
    pub fn new() -> Self {
        let identity = NEXT_SOURCE_ID.with(|next| {
            let identity = next.get();
            next.set(identity.wrapping_add(1).max(1));
            SourceIdentity(identity)
        });
        let previous = CURRENT_SOURCE.with(|current| current.replace(Some(identity)));
        Self {
            previous,
            identity,
            _not_send: PhantomData,
        }
    }

    /// Attach this fragment's identity to one parser-local byte offset.
    pub fn position(&self, byte_offset: usize) -> SourcePosition {
        let active = CURRENT_SOURCE.with(Cell::get);
        assert_eq!(
            active,
            Some(self.identity),
            "SourceScope::position requires this scope to be active"
        );
        SourcePosition {
            source: self.identity,
            byte_offset,
        }
    }
}

impl Default for SourceScope {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for SourceScope {
    fn drop(&mut self) {
        CURRENT_SOURCE.with(|current| current.set(self.previous));
    }
}

/// Attach the active parsed fragment's identity to a parser-local byte offset.
/// Returns `None` outside a [`SourceScope`], where source ordering is undefined.
pub fn current_source_position(byte_offset: usize) -> Option<SourcePosition> {
    CURRENT_SOURCE.with(|current| {
        current.get().map(|source| SourcePosition {
            source,
            byte_offset,
        })
    })
}

/// Scoped lexical module path used to select owner-exact aliases. Pre-scan
/// discovery order must never substitute for module visibility (Issue #11086).
pub struct ModuleScope {
    previous_len: usize,
}

impl ModuleScope {
    pub fn new(name: &str) -> Self {
        let previous_len = CURRENT_MODULE.with(|module| {
            let mut module = module.borrow_mut();
            let previous_len = module.len();
            module.push(name.to_string());
            previous_len
        });
        Self { previous_len }
    }
}

impl Drop for ModuleScope {
    fn drop(&mut self) {
        CURRENT_MODULE.with(|module| module.borrow_mut().truncate(self.previous_len));
    }
}

/// Read the currently active lexical module path (Issue #11321). Mirrors the
/// ambient state `ModuleScope` maintains, for a caller (a `catch` clause's
/// real lowering, not the whole-program pre-scan) that needs to register an
/// alias entry under the same owner a later signature lookup will compute.
pub fn current_module_owner() -> Vec<String> {
    CURRENT_MODULE.with(|module| module.borrow().clone())
}

/// Marks a method signature that is lowered from inside a function body. Such
/// a definition executes later, so its alias binding is selected from the
/// runtime-visible (canonical latest) state instead of its lexical byte offset.
pub struct RuntimeSignatureScope;

impl RuntimeSignatureScope {
    pub fn new() -> Self {
        RUNTIME_SIGNATURE_DEPTH.with(|depth| depth.set(depth.get().saturating_add(1)));
        Self
    }
}

impl Default for RuntimeSignatureScope {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for RuntimeSignatureScope {
    fn drop(&mut self) {
        RUNTIME_SIGNATURE_DEPTH.with(|depth| depth.set(depth.get().saturating_sub(1)));
    }
}

/// Record `name` as a type declared by the program being lowered
/// (Issue #11104). Called by the type-alias pre-scan for every `struct`,
/// `mutable struct`, `abstract type` and `primitive type` head it walks,
/// including inside module bodies.
pub fn register_declared_type(name: &str) {
    if name.is_empty() {
        return;
    }
    DECLARED_TYPES.with(|d| {
        d.borrow_mut().insert(name.to_string());
    });
}

/// True when `name` was declared as a type by the program being lowered.
/// Matches the qualified spelling and its leaf (`M.Point` / `Point`), since a
/// module-level declaration is visible under both.
pub fn is_declared_type(name: &str) -> bool {
    DECLARED_TYPES.with(|d| {
        let declared = d.borrow();
        if declared.contains(name) {
            return true;
        }
        let leaf = name.rsplit('.').next().unwrap_or(name);
        leaf != name && declared.contains(leaf)
    })
}

/// True when `name` is currently shadowed by a scoped type parameter.
fn is_scoped_excluded(name: &str) -> bool {
    crate::lowering::type_binder_env::contains(name)
}

/// True when `name` is an active lexically scoped type parameter (a
/// `where`-clause binder of the signature currently being parsed). Used by
/// annotation parsing so a binder whose name collides with a builtin/global
/// type name (`h(x::Float64) where {Float64}`) still lowers the annotation as
/// the method-local TypeVar, not the builtin concrete type — upstream Julia
/// scopes the fresh TypeVar over the whole method signature (Issue #10407).
pub fn is_scoped_type_param(name: &str) -> bool {
    is_scoped_excluded(name)
}

fn alias_name_is_excluded(name: &str, excluded: &[String]) -> bool {
    let leaf = name.rsplit('.').next().unwrap_or(name);
    excluded.iter().any(|e| e == name || e == leaf)
        || is_scoped_excluded(name)
        || is_scoped_excluded(leaf)
}

#[derive(Debug, Clone, Copy)]
enum ResolutionMode {
    Canonical,
    Signature(SourcePosition),
}

fn split_owner_and_leaf(name: &str) -> (Vec<String>, &str) {
    let Some((owner, leaf)) = name.rsplit_once('.') else {
        return (Vec::new(), name);
    };
    (owner.split('.').map(str::to_string).collect(), leaf)
}

/// Record that `importing_scope` imports `imported_path` (`using .M` /
/// `using .M: names` / `import .M: names`), so `M`-owned aliases stay
/// bare-visible inside that scope (Issue #11452).
pub fn register_import_edge(importing_scope: &[String], imported_path: &[String]) {
    if imported_path.is_empty() {
        return;
    }
    IMPORT_EDGES.with(|edges| {
        edges
            .borrow_mut()
            .insert((importing_scope.to_vec(), imported_path.to_vec()));
    });
}

/// Whether an alias owned by `entry_owner` may resolve a BARE (unqualified)
/// use inside `active_owner` through the unique-leaf fallback. Visible owners
/// are the top level, lexically enclosing modules, and modules imported
/// (`using`/`import`) by the active scope or one of its lexical ancestors.
/// A never-imported sibling module's alias must not leak (Issue #11452).
fn owner_visible_for_bare_use(entry_owner: &[String], active_owner: &[String]) -> bool {
    if entry_owner.is_empty() || active_owner.starts_with(entry_owner) {
        return true;
    }
    IMPORT_EDGES.with(|edges| {
        edges.borrow().iter().any(|(scope, imported)| {
            active_owner.starts_with(scope)
                && (entry_owner.ends_with(imported) || imported.ends_with(entry_owner))
        })
    })
}

fn entry_is_available(entry: &AliasEntry, mode: ResolutionMode) -> bool {
    match (mode, entry.origin) {
        (ResolutionMode::Signature(use_position), Some(definition_position)) => {
            definition_position.definition_is_visible_at(use_position)
        }
        _ => true,
    }
}

fn newest_entry<'a>(entries: impl Iterator<Item = &'a AliasEntry>) -> Option<&'a AliasEntry> {
    entries.max_by_key(|entry| entry.registration_order)
}

fn resolve_alias_entry(
    name: &str,
    excluded: &[String],
    mode: ResolutionMode,
    active_owner: &[String],
) -> Option<AliasEntry> {
    if alias_name_is_excluded(name, excluded) {
        return None;
    }

    ALIASES.with(|a| {
        let aliases = a.borrow();
        let (requested_owner, leaf) = split_owner_and_leaf(name);
        if alias_name_is_excluded(leaf, excluded) {
            return None;
        }
        let entries = aliases.get(leaf)?;

        if !requested_owner.is_empty() {
            if entries.iter().any(|entry| entry.owner == requested_owner) {
                let selected = newest_entry(
                    entries
                        .iter()
                        .filter(|entry| entry.owner == requested_owner)
                        .filter(|entry| entry_is_available(entry, mode)),
                );
                return selected.filter(|entry| entry.is_alias).cloned();
            }
        } else {
            if entries.iter().any(|entry| entry.owner == active_owner) {
                let selected = newest_entry(
                    entries
                        .iter()
                        .filter(|entry| entry.owner.as_slice() == active_owner)
                        .filter(|entry| entry_is_available(entry, mode)),
                );
                return selected.filter(|entry| entry.is_alias).cloned();
            }
        }

        // Preserve the historical unique-leaf fallback used by imported and
        // qualified aliases, but compare one newest AVAILABLE entry per owner.
        // Sibling-module pre-scan order can therefore never become visibility.
        let mut per_owner: HashMap<&[String], &AliasEntry> = HashMap::new();
        for entry in entries
            .iter()
            .filter(|entry| entry_is_available(entry, mode))
            .filter(|entry| {
                !requested_owner.is_empty()
                    || (entry.bare_fallback
                        && owner_visible_for_bare_use(&entry.owner, active_owner))
            })
        {
            let slot = per_owner.entry(entry.owner.as_slice()).or_insert(entry);
            if entry.registration_order > slot.registration_order {
                *slot = entry;
            }
        }

        let mut unique: Option<&AliasEntry> = None;
        for entry in per_owner.values().copied() {
            if unique.is_some_and(|existing| {
                existing.params != entry.params || existing.target != entry.target
            }) {
                return None;
            }
            unique = Some(match unique {
                Some(existing) if existing.registration_order > entry.registration_order => {
                    existing
                }
                _ => entry,
            });
        }
        unique.filter(|entry| entry.is_alias).cloned()
    })
}

/// True when `name` is already registered as a type alias in the current
/// lowering pass. Lets the alias gate accept an alias OF an alias
/// (`const AE = E; const BE = AE`) as a type name (Issue #11104).
pub fn is_registered_alias(name: &str) -> bool {
    if is_empty() {
        return false;
    }
    let active_owner = CURRENT_MODULE.with(|module| module.borrow().clone());
    resolve_alias_entry(name, &[], ResolutionMode::Canonical, &active_owner).is_some()
}

/// Number of registered aliases. Used by the pre-scan to iterate its binding
/// walk to a fixpoint, so an alias chain resolves regardless of source order.
pub fn registered_count() -> usize {
    ALIASES.with(|a| a.borrow().len())
}

/// Register a type alias for the current lowering pass.
pub fn register(name: &str, params: Vec<String>, target: &str) {
    let (owner, leaf) = split_owner_and_leaf(name);
    let bare_fallback = owner.is_empty();
    register_entry(leaf, params, target, owner, None, true, bare_fallback);
}

/// Register a qualified module alias that remains available as `M.A` but must
/// not make bare `A` visible merely because it is the only matching leaf.
pub fn register_qualified_only(name: &str, params: Vec<String>, target: &str) {
    let (owner, leaf) = split_owner_and_leaf(name);
    register_entry(leaf, params, target, owner, None, true, false);
}

/// Register an alias discovered by the current source's pre-scan. The lexical
/// owner is explicit so sibling modules with the same leaf retain independent
/// entries, and the source origin enables signature-order filtering.
pub fn register_prescanned(
    name: &str,
    params: Vec<String>,
    target: &str,
    definition_position: SourcePosition,
    owner: &[String],
) {
    register_entry(
        name,
        params,
        target,
        owner.to_vec(),
        Some(definition_position),
        true,
        true,
    );
}

/// Record a value rebinding in the alias history. It acts as an owner-local
/// tombstone from its source position onward, while leaving an earlier alias
/// visible to signatures that execute before the rebinding.
pub fn register_prescanned_non_alias(
    name: &str,
    definition_position: SourcePosition,
    owner: &[String],
) {
    register_entry(
        name,
        Vec::new(),
        "",
        owner.to_vec(),
        Some(definition_position),
        false,
        true,
    );
}

fn register_entry(
    leaf: &str,
    params: Vec<String>,
    target: &str,
    owner: Vec<String>,
    origin: Option<SourcePosition>,
    is_alias: bool,
    bare_fallback: bool,
) {
    let registration_order = NEXT_REGISTRATION_ORDER.with(|next| {
        let order = next.get();
        next.set(order.wrapping_add(1).max(1));
        order
    });
    ALIASES.with(|a| {
        a.borrow_mut()
            .entry(leaf.to_string())
            .or_default()
            .push(AliasEntry {
                owner,
                params,
                target: target.to_string(),
                origin,
                registration_order,
                is_alias,
                bare_fallback,
            });
    });
}

/// True when `name` resolves to a non-parametric alias whose target is
/// already an APPLIED type (`w = Plain{Int64}` registers `w ->
/// "Plain{Int64}"`). Using such a name with a further parameter list
/// (`w{Float64}`) is a chained `UnionAll` application that must be evaluated
/// at runtime through `ApplyTypeDynamic` — static expansion cannot append the
/// new arguments to the partially-applied target, and used to silently DROP
/// them (`w{Float64}` -> `Plain{Int64}`). Consulted by the lowering's
/// parametrized-type base classification (Issue #10643).
pub fn is_applied_type_alias(name: &str) -> bool {
    if is_empty() {
        return false;
    }
    let active_owner = CURRENT_MODULE.with(|module| module.borrow().clone());
    resolve_alias_entry(name, &[], ResolutionMode::Canonical, &active_owner)
        .is_some_and(|entry| entry.params.is_empty() && entry.target.contains('{'))
}

/// Record that `name` is bound at runtime to a type value (e.g. the `UnionAll`
/// of a `where`-clause parametric alias, Issue #10372) instead of being a
/// static string alias. Consulted by the alias extraction in
/// `lowering::stmt::extract_type_alias_from_binding` so a bare-identifier
/// assignment applying such a name (`z = MyVec{Float64}`) lowers as an
/// ordinary runtime assignment (Issue #10501).
pub fn register_runtime_type_binding(name: &str) {
    RUNTIME_TYPE_BINDINGS.with(|r| {
        r.borrow_mut().insert(name.to_string());
    });
}

/// True when `name` was recorded as a runtime type binding by
/// [`register_runtime_type_binding`] during the current lowering pass.
pub fn is_runtime_type_binding(name: &str) -> bool {
    RUNTIME_TYPE_BINDINGS.with(|r| r.borrow().contains(name))
}

/// Clear all registered aliases. Used by tests and at the very start of a
/// top-level lowering pass.
pub fn clear() {
    ALIASES.with(|a| a.borrow_mut().clear());
    RUNTIME_TYPE_BINDINGS.with(|r| r.borrow_mut().clear());
    DECLARED_TYPES.with(|d| d.borrow_mut().clear());
    IMPORT_EDGES.with(|e| e.borrow_mut().clear());
}

/// An opaque snapshot of the alias table, captured before a (possibly nested)
/// lowering pass and restored afterwards. This keeps nested lowering — such as
/// the stdlib/module loading triggered by `using Test` — from destroying the
/// aliases registered by the enclosing program (Issue #5055).
pub struct AliasScope {
    saved: HashMap<String, Vec<AliasEntry>>,
    saved_runtime: HashSet<String>,
    saved_declared: HashSet<String>,
    saved_imports: HashSet<(Vec<String>, Vec<String>)>,
}

/// Snapshot the current alias table, returning a guard whose `restore` call (or
/// drop) reinstates the captured state. The table itself is left intact so the
/// caller can immediately register additional aliases on top of it.
pub fn snapshot() -> AliasScope {
    let saved = ALIASES.with(|a| a.borrow().clone());
    let saved_runtime = RUNTIME_TYPE_BINDINGS.with(|r| r.borrow().clone());
    let saved_declared = DECLARED_TYPES.with(|d| d.borrow().clone());
    let saved_imports = IMPORT_EDGES.with(|e| e.borrow().clone());
    AliasScope {
        saved,
        saved_runtime,
        saved_declared,
        saved_imports,
    }
}

impl AliasScope {
    /// Restore the snapshotted alias table, discarding aliases registered since
    /// the snapshot was taken.
    pub fn restore(self) {
        ALIASES.with(|a| *a.borrow_mut() = self.saved);
        RUNTIME_TYPE_BINDINGS.with(|r| *r.borrow_mut() = self.saved_runtime);
        DECLARED_TYPES.with(|d| *d.borrow_mut() = self.saved_declared);
        IMPORT_EDGES.with(|e| *e.borrow_mut() = self.saved_imports);
    }
}

/// True when any aliases are registered (cheap early-out for the common case).
fn is_empty() -> bool {
    ALIASES.with(|a| a.borrow().is_empty())
}

/// Split a type name string into `(base, Some(args))` where `args` are the
/// top-level comma-separated parameters inside the outermost `{...}`. Returns
/// `(name, None)` for a bare name. Respects nesting so `Foo{Bar{T}, S}` yields
/// `["Bar{T}", "S"]`.
fn split_curly(name: &str) -> (&str, Option<Vec<String>>) {
    let Some(open) = name.find('{') else {
        return (name, None);
    };
    if !name.ends_with('}') {
        return (name, None);
    }
    let base = name[..open].trim();
    let inner = &name[open + 1..name.len() - 1];
    let mut args = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (i, &b) in inner.as_bytes().iter().enumerate() {
        match b {
            b'{' => depth += 1,
            b'}' => depth = depth.saturating_sub(1),
            b',' if depth == 0 => {
                args.push(inner[start..i].trim().to_string());
                start = i + 1;
            }
            _ => {}
        }
    }
    args.push(inner[start..].trim().to_string());
    (base, Some(args))
}

/// Substitute positional parameters into a target template. For template
/// `"Vector{T}"` with params `["T"]` and args `["Int"]`, yields `"Vector{Int}"`.
/// Substitution is whole-token aware so `T` does not match inside `Tuple`.
fn substitute(template: &str, params: &[String], args: &[String]) -> String {
    if params.is_empty() {
        return template.to_string();
    }
    let mut result = template.to_string();
    for (param, arg) in params.iter().zip(args.iter()) {
        result = replace_token(&result, param, arg);
    }
    result
}

/// Replace whole-identifier occurrences of `from` with `to` in `s`.
fn replace_token(s: &str, from: &str, to: &str) -> String {
    if from.is_empty() {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let from_bytes = from.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i..].starts_with(from_bytes) {
            let before_ok = i == 0 || !is_ident_byte(bytes[i - 1]);
            let after_idx = i + from_bytes.len();
            let after_ok = after_idx >= bytes.len() || !is_ident_byte(bytes[after_idx]);
            if before_ok && after_ok {
                out.push_str(to);
                i = after_idx;
                continue;
            }
        }
        // Copy one UTF-8 character to avoid splitting multibyte sequences.
        let ch_len = utf8_len(bytes[i]);
        let end = (i + ch_len).min(s.len());
        out.push_str(&s[i..end]);
        i = end;
    }
    out
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'!'
}

fn utf8_len(first: u8) -> usize {
    if first < 0x80 {
        1
    } else if first >> 5 == 0b110 {
        2
    } else if first >> 4 == 0b1110 {
        3
    } else {
        4
    }
}

/// Expand a type-name string by resolving any registered alias at its base.
/// Recursively expands the alias target and any aliases used in arguments, so
/// nested aliases (`Foo{Bar{Int}}`) resolve fully. Non-alias names are returned
/// unchanged. Bounded recursion guards against pathological self-referential
/// definitions.
pub fn expand(name: &str) -> String {
    if is_empty() {
        return name.to_string();
    }
    let active_owner = CURRENT_MODULE.with(|module| module.borrow().clone());
    expand_bounded(name, 16, &[], ResolutionMode::Canonical, &active_owner)
}

/// Expand a type annotation as visible where its method definition executes.
/// Only a selected alias from the SAME source identity is ordered by byte
/// position; aliases from includes, packages, or prior REPL fragments never
/// compare their unrelated offsets (Issue #11086).
pub fn expand_for_signature(name: &str, use_position: SourcePosition) -> String {
    if is_empty() {
        return name.to_string();
    }
    let active_owner = CURRENT_MODULE.with(|module| module.borrow().clone());
    if RUNTIME_SIGNATURE_DEPTH.with(|depth| depth.get() > 0) {
        return expand_bounded(name, 16, &[], ResolutionMode::Canonical, &active_owner);
    }
    expand_bounded(
        name,
        16,
        &[],
        ResolutionMode::Signature(use_position),
        &active_owner,
    )
}

/// Like [`expand`], but treats every name in `excluded` as if it were *not* a
/// registered alias, leaving those tokens verbatim. This keeps a construct's own
/// lexically scoped type parameters (e.g. the `T` in `struct Wrap{T} <:
/// AbstractVector{T}`) from being frozen to the value of a same-named top-level
/// global/alias (`T = Int64`). Upstream Julia scopes a struct's type parameters
/// to the struct, so a global of the same name is irrelevant when lowering the
/// declared parent type (Issue #7840).
pub fn expand_excluding(name: &str, excluded: &[String]) -> String {
    // Even when no aliases are registered we keep the early-out cheap; the
    // exclusion set only matters when an alias would otherwise be substituted.
    if is_empty() {
        return name.to_string();
    }
    let active_owner = CURRENT_MODULE.with(|module| module.borrow().clone());
    expand_bounded(name, 16, excluded, ResolutionMode::Canonical, &active_owner)
}

fn expand_bounded(
    name: &str,
    fuel: usize,
    excluded: &[String],
    mode: ResolutionMode,
    active_owner: &[String],
) -> String {
    if fuel == 0 {
        return name.to_string();
    }
    let (base, args) = split_curly(name);

    // First, recursively expand any aliases appearing in the arguments.
    let expanded_args: Option<Vec<String>> = args.map(|a| {
        a.iter()
            .map(|arg| expand_bounded(arg, fuel - 1, excluded, mode, active_owner))
            .collect()
    });

    // A name shadowed by an enclosing construct's own type parameter is not an
    // alias here, regardless of any same-named global binding. The exclusion may
    // be an explicit `excluded` argument (struct parameters, Issue #7840) or a
    // thread-local scoped exclusion pushed around signature parsing (method
    // `where` parameters, Issue #7847).
    let entry = resolve_alias_entry(base, excluded, mode, active_owner);
    let Some(entry) = entry else {
        // Not an alias: reassemble base with expanded args (args may have
        // contained aliases that were expanded above).
        return match expanded_args {
            Some(args) => format!("{}{{{}}}", base, args.join(", ")),
            None => base.to_string(),
        };
    };

    match &expanded_args {
        // Parametric use `Alias{A, B}`: positionally substitute into target.
        Some(args) if !entry.params.is_empty() => {
            let substituted = substitute(&entry.target, &entry.params, args);
            // The target may itself name another alias (`MyVec{T} = MyArr{T}`).
            expand_bounded(&substituted, fuel - 1, excluded, mode, &entry.owner)
        }
        // Bare alias use `Alias` for a parametric alias: in upstream this is the
        // UnionAll itself; the bare target base (`Vector`) is the closest static
        // analogue.
        None if !entry.params.is_empty() => {
            let (target_base, _) = split_curly(&entry.target);
            expand_bounded(target_base, fuel - 1, excluded, mode, &entry.owner)
        }
        // Non-parametric alias used with explicit args (rare/invalid): swap base.
        Some(args) => {
            let (target_base, target_args) = split_curly(&entry.target);
            match target_args {
                Some(_) => expand_bounded(&entry.target, fuel - 1, excluded, mode, &entry.owner),
                None => format!("{}{{{}}}", target_base, args.join(", ")),
            }
        }
        // Non-parametric alias used bare: expand to its (recursively resolved)
        // target.
        None => expand_bounded(&entry.target, fuel - 1, excluded, mode, &entry.owner),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_clean<T>(f: impl FnOnce() -> T) -> T {
        clear();
        let r = f();
        clear();
        r
    }

    #[test]
    fn parametric_alias_substitutes_positional() {
        with_clean(|| {
            register("MyVec", vec!["T".to_string()], "Vector{T}");
            assert_eq!(expand("MyVec{Int}"), "Vector{Int}");
            assert_eq!(expand("MyVec{Float64}"), "Vector{Float64}");
        });
    }

    #[test]
    fn bare_parametric_alias_expands_to_base() {
        with_clean(|| {
            register("MyVec", vec!["T".to_string()], "Vector{T}");
            assert_eq!(expand("MyVec"), "Vector");
        });
    }

    #[test]
    fn non_parametric_alias_swaps_name() {
        with_clean(|| {
            register("IntVec", vec![], "Vector{Int}");
            assert_eq!(expand("IntVec"), "Vector{Int}");
        });
    }

    #[test]
    fn token_substitution_does_not_match_substrings() {
        with_clean(|| {
            register("Pair2", vec!["T".to_string()], "Tuple{T, T}");
            // `T` must not corrupt `Tuple`.
            assert_eq!(expand("Pair2{Int}"), "Tuple{Int, Int}");
        });
    }

    #[test]
    fn multi_param_substitution() {
        with_clean(|| {
            register(
                "MyDict",
                vec!["K".to_string(), "V".to_string()],
                "Dict{K, V}",
            );
            assert_eq!(expand("MyDict{String, Int}"), "Dict{String, Int}");
        });
    }

    #[test]
    fn nested_alias_in_argument_expands() {
        with_clean(|| {
            register("MyVec", vec!["T".to_string()], "Vector{T}");
            // Vector{MyVec{Int}} -> Vector{Vector{Int}}
            assert_eq!(expand("Vector{MyVec{Int}}"), "Vector{Vector{Int}}");
        });
    }

    #[test]
    fn alias_chain_resolves() {
        with_clean(|| {
            register("MyArr", vec!["T".to_string()], "Vector{T}");
            register("MyVec", vec!["T".to_string()], "MyArr{T}");
            assert_eq!(expand("MyVec{Int}"), "Vector{Int}");
        });
    }

    #[test]
    fn unknown_name_unchanged() {
        with_clean(|| {
            register("MyVec", vec!["T".to_string()], "Vector{T}");
            assert_eq!(expand("Foo{Bar}"), "Foo{Bar}");
            assert_eq!(expand("Int"), "Int");
        });
    }

    #[test]
    fn excluded_param_shadows_same_named_global_alias() {
        // Issue #7840: a top-level `T = Int64` registers a non-parametric alias
        // `T -> Int64`. When lowering a struct's parent `AbstractVector{T}`, the
        // struct's own param `T` must shadow that alias so the parametric
        // template is preserved instead of frozen to `AbstractVector{Int64}`.
        with_clean(|| {
            register("T", vec![], "Int64");
            assert_eq!(expand("AbstractVector{T}"), "AbstractVector{Int64}");
            assert_eq!(
                expand_excluding("AbstractVector{T}", &["T".to_string()]),
                "AbstractVector{T}"
            );
        });
    }

    #[test]
    fn excluded_does_not_block_unrelated_aliases() {
        // Excluding `T` must not stop a genuinely different alias from resolving.
        with_clean(|| {
            register("T", vec![], "Int64");
            register("MyVec", vec!["S".to_string()], "Vector{S}");
            assert_eq!(
                expand_excluding("MyVec{T}", &["T".to_string()]),
                "Vector{T}"
            );
            // Non-excluded alias still resolves even with a non-empty excl set.
            assert_eq!(
                expand_excluding("MyVec{Int}", &["T".to_string()]),
                "Vector{Int}"
            );
        });
    }

    #[test]
    fn qualified_alias_expands_by_unique_leaf_issue_8406() {
        // Module-qualified where bounds such as `T <: AbstractAlgebra.RingElement`
        // must resolve through the same alias entry as the unqualified
        // `RingElement` when the leaf is unique in the current lowering scope.
        with_clean(|| {
            register(
                "RingElement",
                vec![],
                "Union{RingElem, Integer, Rational, AbstractFloat}",
            );
            assert_eq!(
                expand("AbstractAlgebra.RingElement"),
                "Union{RingElem, Integer, Rational, AbstractFloat}"
            );
        });
    }

    #[test]
    fn qualified_alias_leaf_ambiguity_stays_unchanged_issue_8406() {
        with_clean(|| {
            register("A.RingElement", vec![], "Int64");
            register("B.RingElement", vec![], "Float64");
            assert_eq!(expand("C.RingElement"), "C.RingElement");
            assert_eq!(expand("A.RingElement"), "Int64");
        });
    }

    #[test]
    fn scoped_exclusion_shadows_alias_within_scope_only() {
        // Issue #7847: a method `where T` parameter must shadow a same-named
        // top-level alias (`T = Int64`) while its signature is parsed. The
        // The shared binder scope makes the plain `expand` (used by the
        // signature parser) treat `T` as a type variable for its lifetime, then
        // restores normal alias resolution on drop.
        with_clean(|| {
            register("T", vec![], "Int64");
            // Outside any scope, the global alias wins (top-level `x::T` uses).
            assert_eq!(expand("Tuple{T, Int64}"), "Tuple{Int64, Int64}");
            {
                let _g = crate::lowering::type_binder_env::Scope::from_names(&["T".to_string()]);
                // Inside the scope, `T` is left verbatim as a type variable.
                assert_eq!(expand("Tuple{T, Int64}"), "Tuple{T, Int64}");
                // An unrelated name is unaffected by the exclusion.
                assert_eq!(expand("Int64"), "Int64");
            }
            // After the guard drops, normal resolution resumes.
            assert_eq!(expand("Tuple{T, Int64}"), "Tuple{Int64, Int64}");
        });
    }

    #[test]
    fn scoped_exclusion_nests_and_pops() {
        // Nested scopes union their excluded names and each pops independently.
        with_clean(|| {
            register("T", vec![], "Int64");
            register("S", vec![], "Float64");
            {
                let _outer =
                    crate::lowering::type_binder_env::Scope::from_names(&["T".to_string()]);
                assert_eq!(expand("S"), "Float64"); // S not excluded yet
                {
                    let _inner =
                        crate::lowering::type_binder_env::Scope::from_names(&["S".to_string()]);
                    assert_eq!(expand("T"), "T");
                    assert_eq!(expand("S"), "S");
                }
                // Inner scope dropped: S resolves again, T still excluded.
                assert_eq!(expand("S"), "Float64");
                assert_eq!(expand("T"), "T");
            }
            assert_eq!(expand("T"), "Int64");
        });
    }

    #[test]
    fn scoped_exclusion_empty_is_inert() {
        // An empty name set must not push a scope (no effect on resolution).
        with_clean(|| {
            register("T", vec![], "Int64");
            let _g = crate::lowering::type_binder_env::Scope::from_names(&[]);
            assert_eq!(expand("T"), "Int64");
        });
    }

    #[test]
    fn runtime_type_bindings_register_and_clear_issue_10501() {
        // A `where`-clause parametric alias (`MyVec{T} = Vector{T} where
        // T<:Real`) is a RUNTIME `UnionAll` binding, recorded so that a later
        // bare-identifier application assignment (`z = MyVec{Float64}`)
        // declines static alias extraction (Issue #10501).
        with_clean(|| {
            assert!(!is_runtime_type_binding("MyVec"));
            register_runtime_type_binding("MyVec");
            assert!(is_runtime_type_binding("MyVec"));
            assert!(!is_runtime_type_binding("Other"));
        });
        // `with_clean` ran `clear()` on exit: the set must be empty again.
        assert!(!is_runtime_type_binding("MyVec"));
    }

    #[test]
    fn snapshot_restores_runtime_type_bindings_issue_10501() {
        // Nested lowering passes (e.g. the stdlib load triggered by `using
        // Test`) snapshot/restore the alias state; the runtime-binding set
        // must round-trip with it so an inner pass cannot leak or destroy the
        // enclosing program's runtime type bindings.
        with_clean(|| {
            register_runtime_type_binding("Outer");
            let scope = snapshot();
            register_runtime_type_binding("Inner");
            assert!(is_runtime_type_binding("Outer"));
            assert!(is_runtime_type_binding("Inner"));
            scope.restore();
            assert!(is_runtime_type_binding("Outer"));
            assert!(!is_runtime_type_binding("Inner"));
        });
    }
}

#[cfg(test)]
mod source_order_tests_11086 {
    use super::*;

    fn with_clean<T>(f: impl FnOnce() -> T) -> T {
        clear();
        let result = f();
        clear();
        result
    }

    #[test]
    fn source_position_requires_an_active_source_scope_11100() {
        with_clean(|| {
            assert_eq!(current_source_position(7), None);
            {
                let source = SourceScope::new();
                assert_eq!(current_source_position(7), Some(source.position(7)));
            }
            assert_eq!(current_source_position(7), None);
        });
    }

    #[test]
    fn outer_scope_cannot_mint_a_position_while_inner_scope_is_active_11100() {
        with_clean(|| {
            let result = std::panic::catch_unwind(|| {
                let outer = SourceScope::new();
                let _inner = SourceScope::new();
                outer.position(7);
            });
            assert!(result.is_err());
            assert_eq!(current_source_position(7), None);
        });
    }

    #[test]
    fn same_source_later_alias_is_unavailable_to_signature_11086() {
        with_clean(|| {
            let source = SourceScope::new();
            register_prescanned("Later", vec![], "Int64", source.position(100), &[]);
            assert_eq!(expand_for_signature("Later", source.position(50)), "Later");
            assert_eq!(expand_for_signature("Later", source.position(100)), "Int64");
        });
    }

    #[test]
    fn different_source_offsets_are_never_compared_11086() {
        with_clean(|| {
            {
                let source = SourceScope::new();
                register_prescanned("Prior", vec![], "Int64", source.position(10_000), &[]);
            }
            {
                let source = SourceScope::new();
                assert_eq!(expand_for_signature("Prior", source.position(1)), "Int64");
            }
        });
    }

    #[test]
    fn unavailable_redefinition_falls_back_to_prior_source_entry_11086() {
        with_clean(|| {
            {
                let source = SourceScope::new();
                register_prescanned("Alias", vec![], "Int64", source.position(500), &[]);
            }
            {
                let source = SourceScope::new();
                register_prescanned("Alias", vec![], "Float64", source.position(100), &[]);
                assert_eq!(expand_for_signature("Alias", source.position(50)), "Int64");
                assert_eq!(
                    expand_for_signature("Alias", source.position(100)),
                    "Float64"
                );
            }
        });
    }

    #[test]
    fn same_leaf_aliases_select_the_active_module_owner_11086() {
        with_clean(|| {
            let source = SourceScope::new();
            register_prescanned("Shared", vec![], "String", source.position(10), &[]);
            register_prescanned(
                "Shared",
                vec![],
                "Int64",
                source.position(20),
                &["A".to_string()],
            );
            register_prescanned(
                "Shared",
                vec![],
                "Float64",
                source.position(30),
                &["B".to_string()],
            );
            register_prescanned("Nested", vec![], "Shared", source.position(40), &[]);
            register_prescanned(
                "Nested",
                vec![],
                "Shared",
                source.position(50),
                &["A".to_string()],
            );
            register_prescanned(
                "Nested",
                vec![],
                "Shared",
                source.position(60),
                &["B".to_string()],
            );

            assert_eq!(
                expand_for_signature("Shared", source.position(70)),
                "String"
            );
            assert_eq!(
                expand_for_signature("Nested", source.position(70)),
                "String"
            );
            {
                let _module = ModuleScope::new("A");
                assert_eq!(expand_for_signature("Shared", source.position(70)), "Int64");
                assert_eq!(expand_for_signature("Nested", source.position(70)), "Int64");
                assert_eq!(expand("Shared"), "Int64");
            }
            {
                let _module = ModuleScope::new("B");
                assert_eq!(
                    expand_for_signature("Shared", source.position(70)),
                    "Float64"
                );
                assert_eq!(
                    expand_for_signature("Nested", source.position(70)),
                    "Float64"
                );
            }
            assert_eq!(expand("A.Shared"), "Int64");
            assert_eq!(expand("B.Shared"), "Float64");
            assert_eq!(expand("A.Nested"), "Int64");
            assert_eq!(expand("B.Nested"), "Float64");
        });
    }

    #[test]
    fn unavailable_owner_entry_does_not_fall_back_to_main_alias_11086() {
        with_clean(|| {
            let source = SourceScope::new();
            register_prescanned("Shared", vec![], "String", source.position(10), &[]);
            register_prescanned(
                "Shared",
                vec![],
                "Int64",
                source.position(100),
                &["A".to_string()],
            );
            let _module = ModuleScope::new("A");
            assert_eq!(
                expand_for_signature("Shared", source.position(50)),
                "Shared"
            );
        });
    }

    #[test]
    fn value_rebinding_is_a_source_ordered_alias_tombstone_11086() {
        with_clean(|| {
            {
                let source = SourceScope::new();
                register_prescanned("A", vec![], "Int64", source.position(1), &[]);
            }
            {
                let source = SourceScope::new();
                register_prescanned_non_alias("A", source.position(20), &[]);
                assert_eq!(expand_for_signature("A", source.position(10)), "Int64");
                assert_eq!(expand_for_signature("A", source.position(20)), "A");
                assert_eq!(expand("A"), "A");
            }
        });
    }

    #[test]
    fn prior_module_alias_is_qualified_only_without_import_11086() {
        with_clean(|| {
            register_qualified_only("M.A", vec![], "Int64");
            assert_eq!(expand("A"), "A");
            assert_eq!(expand("M.A"), "Int64");
        });
    }

    #[test]
    fn sibling_module_alias_does_not_leak_bare_11452() {
        with_clean(|| {
            let source = SourceScope::new();
            let owners = vec!["AliasOwner".to_string()];
            register_prescanned("BigInt", vec![], "Int64", source.position(10), &owners);
            let _module = ModuleScope::new("Consumer");
            // A never-imported sibling alias stays hidden bare but available qualified.
            assert_eq!(
                expand_for_signature("BigInt", source.position(50)),
                "BigInt"
            );
            assert_eq!(expand("AliasOwner.BigInt"), "Int64");
        });
    }

    #[test]
    fn imported_sibling_alias_stays_visible_bare_11452() {
        with_clean(|| {
            let source = SourceScope::new();
            let owners = vec!["Owner".to_string()];
            register_prescanned(
                "ImportedAlias",
                vec![],
                "UInt8",
                source.position(10),
                &owners,
            );
            register_import_edge(&[], &["Owner".to_string()]);
            assert_eq!(
                expand_for_signature("ImportedAlias", source.position(50)),
                "UInt8"
            );
        });
    }

    #[test]
    fn enclosing_module_alias_stays_visible_bare_11452() {
        with_clean(|| {
            let source = SourceScope::new();
            let owners = vec!["A".to_string()];
            register_prescanned("Outer", vec![], "Int64", source.position(10), &owners);
            let _outer = ModuleScope::new("A");
            let _inner = ModuleScope::new("B");
            assert_eq!(expand_for_signature("Outer", source.position(50)), "Int64");
        });
    }

    #[test]
    fn runtime_signature_uses_latest_binding_not_lexical_offset_11086() {
        with_clean(|| {
            let source = SourceScope::new();
            register_prescanned("A", vec![], "Int64", source.position(10), &[]);
            register_prescanned("A", vec![], "Float64", source.position(30), &[]);
            assert_eq!(expand_for_signature("A", source.position(20)), "Int64");
            let _runtime = RuntimeSignatureScope::new();
            assert_eq!(expand_for_signature("A", source.position(20)), "Float64");
        });
    }
}
