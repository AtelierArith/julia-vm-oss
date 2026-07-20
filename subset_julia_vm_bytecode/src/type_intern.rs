//! Session-scoped concrete-type interning registry (Issue #9197, slice 1).
//!
//! This is the **foundation only** for the interned-type-ID epic: it hands each
//! *concrete* Julia type a stable, dense [`ConcreteTypeId`] so that runtime
//! dispatch identity can move off type-name strings and unverified `u64` hashes.
//! See [`docs/vm/TYPE_INTERNING.md`](../../docs/vm/TYPE_INTERNING.md) for
//! the full design (key structure, invalidation, REPL boundary, per-slice
//! consumers).
//!
//! # Why interned ids
//!
//! The struct-table `type_id: usize` a value carries is a *compiler definition
//! index*, not a concrete-type identity: `NewStruct` refines an instance's
//! `struct_name` from runtime field values while keeping the definition's
//! `type_id`, so one `type_id` can dispatch as `SubArray{Int64, 1}` **or**
//! `SubArray{Float64, 2}` (see the `hash_struct_dispatch_identity` doc comment in
//! `subset_julia_vm_vm/src/vm/mod.rs`, which is exactly why the L1 fingerprint hashes
//! the *name string* today rather than `type_id`). A [`ConcreteTypeId`] is the
//! identity `type_id` is not: **distinct type parameters ⇒ distinct id**.
//!
//! # Wired (slice 2)
//!
//! Slice 2 wires [`TypeInternTable::intern`] into the VM as the backing store of
//! the L1 call-site inline cache key (`subset_julia_vm_vm/src/vm/mod.rs`
//! `call_site_arg_type_ids` → `CallSiteCache`), so the module-level
//! `#![allow(dead_code)]` from slice 1 is gone. The read-only / rendering surface
//! (`lookup`, `key`, `display_name`, `len`, `is_empty`, the builders,
//! `ConcreteTypeId::index`) is exercised by the unit tests here but not yet by a
//! production call site — those are consumed by slices S3–S7 (L2 id keys,
//! `parametric_type_args` retirement, typemap, precise invalidation, REPL
//! boundary) and carry a narrow `#[allow(dead_code)]` until then.
//!
//! # Single-threaded VM
//!
//! Per `docs/vm/SINGLE_THREADED_VM.md` the registry is VM-session-local state: a
//! plain owned struct with no `Arc`/`Mutex`/atomics. It is a `Vm` field (slice 2);
//! `!Send`/`!Sync` is expected — a `Struct` key stores the instance's already
//! interned `Rc<str>` `struct_name` so interning a struct key on the dispatch hot
//! path is a refcount bump, not a string re-allocation.

use std::collections::HashMap;
use std::rc::Rc;

/// Interned identity of a *concrete* Julia type within one VM session.
///
/// A dense `u32` handle. Two type spellings map to the same `ConcreteTypeId`
/// **iff** they are the same concrete type *including all type parameters*. Ids
/// are assigned in first-seen order and never reused within a session, so an id
/// handed out early stays valid for the whole session.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct ConcreteTypeId(pub u32);

impl ConcreteTypeId {
    /// The raw dense index behind this id.
    // S3–S7 / unit tests only: the L1 wiring (slice 2) keys on id *equality*, so
    // it never projects an id back to its dense index. Kept for the id→key
    // round-trip that later consumers and the tests exercise.
    #[allow(dead_code)]
    #[inline]
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// The structural description of a concrete type.
///
/// Child types are referenced by their own [`ConcreteTypeId`], so nested
/// parametric types form a DAG and are compared / hashed by id — never by
/// re-parsing a rendered name (retiring the runtime `parametric_type_args` string
/// parsing, Issue #9197 fact 3). The variant set mirrors the dispatch-identity
/// surface `hash_call_site_value_tag` / `hash_struct_dispatch_identity` fold into
/// the L1 fingerprint today, so slice 2 can swap the *hash* for an *exact id
/// sequence* without changing which value kinds participate.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum ConcreteTypeKey {
    /// A sealed primitive concrete type (`Int64`, `Float64`, `Bool`, `String`,
    /// `Nothing`, …). Sealed ⇒ exact-name identity is correct.
    Primitive(Box<str>),
    /// A value type-parameter — the `N` in `Array{T,N}`, the dimensionality in
    /// `SubArray{Int64,1}`, `Val{x}`. Interned like any other type so a
    /// `Struct`'s `params` stay uniformly `Vec<ConcreteTypeId>`.
    IntValue(i64),
    /// A nominal (user or Base) type with **fully resolved** parameters.
    /// `SubArray{Int64,1}` and `SubArray{Float64,2}` differ in `params` ⇒
    /// distinct ids.
    ///
    /// `name` is `Rc<str>` (not `Box<str>`) so slice 2 can clone the interning
    /// key straight from a `StructInstance`'s `struct_name: Rc<str>` with a
    /// refcount bump instead of a heap copy on every struct dispatch. For a
    /// general (non-array-wrapper) struct the L1 layer stores the fully-resolved
    /// dispatch name here with empty `params` (matching today's
    /// `hash_struct_dispatch_identity`, which hashes `struct_name`); structured
    /// `params` decomposition is slice S4.
    Struct {
        name: Rc<str>,
        params: Vec<ConcreteTypeId>,
    },
    /// `Tuple{T1, …}` — element ids in order.
    Tuple(Vec<ConcreteTypeId>),
    /// `NamedTuple{(:a, :b), Tuple{…}}` — field names + element ids.
    NamedTuple {
        names: Vec<Box<str>>,
        params: Vec<ConcreteTypeId>,
    },
    /// The `Array{T,N}` wrapper (`Vector`/`Matrix`/`Array`) — element id + ndims,
    /// mirroring the `(element_type, ndims)` identity
    /// `hash_struct_dispatch_identity` uses for Memory-backed wrappers.
    Array { element: ConcreteTypeId, ndims: u16 },
    /// `Memory{T}`.
    Memory { element: ConcreteTypeId },
    /// `UnitRange{T}` / `StepRange{T,S}` — the exact type parameters and shape
    /// fields the VM uses to derive the range's dispatch name.
    Range {
        element: ConcreteTypeId,
        step: ConcreteTypeId,
        is_float: bool,
        is_step: bool,
    },
    /// A `@enum` type.
    Enum(Box<str>),
    /// A value kind whose runtime dispatch identity is a single **opaque
    /// type-name string** the interner does not (yet) decompose structurally:
    /// the `Type{T}` type-object of a `DataType` (`Type{Int64}`,
    /// `Type{Vector{Float64}}`, …), a function / closure / composed-function
    /// callable singleton (`typeof(f)` / `ComposedFunction`), and the nominal
    /// singleton kinds `Module` / `IOBuffer` / `Base.Generator` / `TypeVar` /
    /// `Xoshiro` / macro-AST nodes / … .
    ///
    /// The string is exactly the pre-#9404 `get_type_name` /
    /// `dynamic_dispatch_type_name` dispatch name, so the interned-id partition
    /// **equals** the retired L2 string-key partition: distinct dispatch names ⇒
    /// distinct ids (this variant never conflates two dispatch-distinct values),
    /// and — being variant-tagged — never collides with a same-spelling
    /// `Struct` / `Enum` / `Primitive` key. Re-caching these kinds (Issue #9427:
    /// they regressed to full re-resolution on every call under S3) is what S5's
    /// "re-cache untracked kinds" pulls forward; structural decomposition of
    /// `Type{T}` remains S4/S5 scope.
    Opaque(Box<str>),
}

/// Session-scoped concrete-type interning registry (Issue #9197).
///
/// Maps [`ConcreteTypeKey`] ⇄ [`ConcreteTypeId`]. Append-only within a session:
/// interning never invalidates an existing id (a new concrete type gets a *new*
/// id). What invalidates on a method-table mutation is the id-*keyed* dispatch
/// caches (via the generation counter today, the #8553/#8554 backedge graph in
/// slice 6), not this table.
#[derive(Debug, Default, Clone)]
pub struct TypeInternTable {
    /// `id.index()` → key. Backs `key` / `display_name` round-trip.
    keys: Vec<ConcreteTypeKey>,
    /// key → id. Backs dedup on `intern` / read-only `lookup`.
    index: HashMap<ConcreteTypeKey, ConcreteTypeId>,
}

impl TypeInternTable {
    /// A fresh, empty registry.
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    /// Intern `key`, returning its stable id. Idempotent: re-interning an already
    /// known key returns the same id and does not grow the table.
    ///
    /// # Panics
    ///
    /// If more than `u32::MAX` distinct concrete types are interned in one
    /// session (not reachable in practice — a session holds thousands, not
    /// billions, of concrete types).
    pub fn intern(&mut self, key: ConcreteTypeKey) -> ConcreteTypeId {
        if let Some(&id) = self.index.get(&key) {
            return id;
        }
        let raw = u32::try_from(self.keys.len())
            .expect("ConcreteTypeId space (u32) exhausted in one VM session");
        let id = ConcreteTypeId(raw);
        self.keys.push(key.clone());
        self.index.insert(key, id);
        id
    }

    /// Read-only probe: the id `key` was interned under, or `None` if it has not
    /// been interned yet.
    // S3 (L2 id-keyed cache) consumer; slice 2 always interns on the dispatch
    // path so it does not probe read-only. Exercised by the unit tests.
    #[allow(dead_code)]
    #[inline]
    pub fn lookup(&self, key: &ConcreteTypeKey) -> Option<ConcreteTypeId> {
        self.index.get(key).copied()
    }

    /// The structural key behind an id, or `None` if the id is not from this
    /// table.
    // S4/S7 (structured param access, REPL boundary) + unit tests: slice 2 stores
    // ids, never reverses them to keys on the hot path.
    #[allow(dead_code)]
    #[inline]
    pub fn key(&self, id: ConcreteTypeId) -> Option<&ConcreteTypeKey> {
        self.keys.get(id.index())
    }

    /// Number of distinct concrete types interned so far.
    // Observability / unit tests; not read by the slice 2 dispatch path.
    #[allow(dead_code)]
    #[inline]
    pub fn len(&self) -> usize {
        self.keys.len()
    }

    /// Whether the registry is empty.
    // Paired with `len` (clippy `len_without_is_empty`); unit tests only for now.
    #[allow(dead_code)]
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    /// Intern a sealed primitive type by name (convenience over [`Self::intern`]).
    #[inline]
    pub fn intern_primitive(&mut self, name: &str) -> ConcreteTypeId {
        self.intern(ConcreteTypeKey::Primitive(name.into()))
    }

    /// Intern a nominal type with already-interned parameter ids (convenience
    /// over [`Self::intern`]).
    // S4 (structured struct params) + unit tests: slice 2 builds `Struct` keys
    // inline so it can clone the instance's `Rc<str>` `struct_name` by refcount
    // bump rather than re-allocate from a `&str`.
    #[allow(dead_code)]
    #[inline]
    pub fn intern_struct(&mut self, name: &str, params: Vec<ConcreteTypeId>) -> ConcreteTypeId {
        self.intern(ConcreteTypeKey::Struct {
            name: name.into(),
            params,
        })
    }

    /// Intern a *rendered concrete type name* (`"SubArray{Int64, 2}"`,
    /// `"Array{Complex{Int64}, 1}"`, `"Dict{String, Int64}"`, `"UnitRange{Int64}"`,
    /// …) into a structured [`ConcreteTypeKey`] DAG, parsing the braces **once** and
    /// referencing every type parameter by its own interned [`ConcreteTypeId`]
    /// (Issue #9197, slice 4).
    ///
    /// This is the sanctioned replacement for the ad-hoc *runtime* type-name string
    /// parsing the epic retires (`split_parametric_args` / `parametric_type_args`,
    /// fact 3): a rendered name is decomposed to structure **exactly once** at
    /// intern time, and thereafter identity is by id — never by re-splitting the
    /// name on every consultation. The recognized parametric families map onto the
    /// structural key variants (`Tuple`, `Array`/`Vector`/`Matrix`, `Memory`,
    /// `UnitRange`/`StepRange`); any other braced base becomes a nominal
    /// `Struct { name, params }` whose parameters are themselves interned
    /// structurally (so `SubArray{Int64, 2}` and `SubArray{Float64, 1}` get
    /// **distinct** ids — the conflation `type_id` cannot express). A bare integer
    /// literal is an `IntValue` value-parameter; an atomic name is a sealed
    /// `Primitive` or a param-less `Struct`.
    ///
    /// The `name → id → key(structural) → display_name(string)` round-trip
    /// (the `intern_type_name_*` unit tests) demonstrates the decomposition loses no
    /// information relative to the type-name string it replaces.
    ///
    /// Landed + proven here (slice 4) as the canonical structural extractor; the
    /// production consumer is slice S5's typemap candidate indexing (the runtime
    /// dispatch RESOLVE path still matches on `JuliaType::Struct(name)` strings,
    /// `vm/dispatch.rs`), so it carries a narrow `#[allow(dead_code)]` until then —
    /// the same "land the capability unwired, wire it in the consuming slice"
    /// pattern S1 used for the registry itself.
    #[allow(dead_code)]
    pub fn intern_type_name(&mut self, name: &str) -> ConcreteTypeId {
        let name = name.trim();
        let Some((base, inner)) = split_parametric_type_name(name) else {
            // Atomic (no `{…}`): an integer literal is a value parameter, a sealed
            // scalar is a primitive, anything else is a param-less nominal type.
            if let Ok(value) = name.parse::<i64>() {
                return self.intern(ConcreteTypeKey::IntValue(value));
            }
            if is_sealed_primitive_name(name) {
                return self.intern(ConcreteTypeKey::Primitive(name.into()));
            }
            return self.intern(ConcreteTypeKey::Struct {
                name: name.into(),
                params: Vec::new(),
            });
        };
        let args = split_top_level_type_args(inner);
        let key = match base {
            "Tuple" => ConcreteTypeKey::Tuple(self.intern_type_name_all(&args)),
            "Vector" => ConcreteTypeKey::Array {
                element: self.intern_first_type_arg(&args),
                ndims: 1,
            },
            "Matrix" => ConcreteTypeKey::Array {
                element: self.intern_first_type_arg(&args),
                ndims: 2,
            },
            "Array" => {
                // `Array{T, N}`: element is the first arg, `N` the trailing rank.
                let element = self.intern_first_type_arg(&args);
                let ndims = args
                    .get(1)
                    .and_then(|arg| arg.trim().parse::<u16>().ok())
                    .unwrap_or(1);
                ConcreteTypeKey::Array { element, ndims }
            }
            "Memory" => ConcreteTypeKey::Memory {
                element: self.intern_first_type_arg(&args),
            },
            "UnitRange" | "OneTo" => ConcreteTypeKey::Range {
                element: self.intern_first_type_arg(&args),
                step: self.intern_first_type_arg(&args),
                is_float: is_float_element_name(args.first().copied()),
                is_step: false,
            },
            "StepRange" | "StepRangeLen" => ConcreteTypeKey::Range {
                element: self.intern_first_type_arg(&args),
                step: args
                    .get(1)
                    .map(|arg| self.intern_type_name(arg))
                    .unwrap_or_else(|| self.intern_first_type_arg(&args)),
                is_float: is_float_element_name(args.first().copied()),
                is_step: true,
            },
            // Every other braced base (`SubArray`, `Dict`, `Complex`, `Set`,
            // user types, …) is a nominal type with structurally-interned params.
            // `NamedTuple` deliberately falls here too — its `(:a, :b)` symbol
            // tuple has no S4/S5 consumer, so it is not special-cased (the
            // dedicated `NamedTuple` key variant is built by the L1 value path,
            // not by name parsing).
            _ => ConcreteTypeKey::Struct {
                name: base.into(),
                params: self.intern_type_name_all(&args),
            },
        };
        self.intern(key)
    }

    /// Intern each top-level type argument structurally, in order.
    #[allow(dead_code)]
    fn intern_type_name_all(&mut self, args: &[&str]) -> Vec<ConcreteTypeId> {
        args.iter().map(|arg| self.intern_type_name(arg)).collect()
    }

    /// Intern the first type argument, or a `Primitive("Any")` for a malformed
    /// empty argument list (`"Vector{}"`), so the caller always gets a child id.
    #[allow(dead_code)]
    fn intern_first_type_arg(&mut self, args: &[&str]) -> ConcreteTypeId {
        match args.first() {
            Some(arg) => self.intern_type_name(arg),
            None => self.intern(ConcreteTypeKey::Primitive("Any".into())),
        }
    }

    /// Render the canonical Julia spelling of an id (`SubArray{Int64, 1}`,
    /// `Array{Complex{Int64}, 1}`, …) by recursively walking child ids.
    ///
    /// The `key → id → key(structural) → display(string)` round-trip is the
    /// demonstration that the structural registry loses no information relative to
    /// the type-name strings it replaces. Returns `None` for an id not from this
    /// table.
    // Diagnostics / id→string round-trip (S4/S7 + unit tests). Slice 2 dispatch
    // never renders an id, so this and its `render_*` helpers are not yet on a
    // production path.
    #[allow(dead_code)]
    pub fn display_name(&self, id: ConcreteTypeId) -> Option<String> {
        let key = self.key(id)?;
        Some(self.render_key(key))
    }

    /// Render an id, falling back to `#<raw>` if it is dangling (cannot happen for
    /// ids produced by this table — children are always interned before parents).
    #[allow(dead_code)]
    fn render_id(&self, id: ConcreteTypeId) -> String {
        self.display_name(id)
            .unwrap_or_else(|| format!("#{}", id.0))
    }

    #[allow(dead_code)]
    fn render_ids(&self, ids: &[ConcreteTypeId]) -> String {
        ids.iter()
            .map(|&child| self.render_id(child))
            .collect::<Vec<_>>()
            .join(", ")
    }

    #[allow(dead_code)]
    fn render_key(&self, key: &ConcreteTypeKey) -> String {
        match key {
            ConcreteTypeKey::Primitive(name)
            | ConcreteTypeKey::Enum(name)
            | ConcreteTypeKey::Opaque(name) => name.to_string(),
            ConcreteTypeKey::IntValue(value) => value.to_string(),
            ConcreteTypeKey::Struct { name, params } => {
                if params.is_empty() {
                    name.to_string()
                } else {
                    format!("{}{{{}}}", name, self.render_ids(params))
                }
            }
            ConcreteTypeKey::Tuple(elements) => {
                format!("Tuple{{{}}}", self.render_ids(elements))
            }
            ConcreteTypeKey::NamedTuple { names, params } => {
                let names_tuple = match names.split_first() {
                    // Julia renders a 1-tuple with a trailing comma: `(:a,)`.
                    Some((only, [])) => format!("(:{only},)"),
                    _ => {
                        let joined = names
                            .iter()
                            .map(|name| format!(":{name}"))
                            .collect::<Vec<_>>()
                            .join(", ");
                        format!("({joined})")
                    }
                };
                format!(
                    "NamedTuple{{{}, Tuple{{{}}}}}",
                    names_tuple,
                    self.render_ids(params)
                )
            }
            ConcreteTypeKey::Array { element, ndims } => match ndims {
                1 => format!("Vector{{{}}}", self.render_id(*element)),
                2 => format!("Matrix{{{}}}", self.render_id(*element)),
                n => format!("Array{{{}, {}}}", self.render_id(*element), n),
            },
            ConcreteTypeKey::Memory { element } => {
                format!("Memory{{{}}}", self.render_id(*element))
            }
            ConcreteTypeKey::Range {
                element,
                step,
                is_step,
                ..
            } => {
                let elem = self.render_id(*element);
                if *is_step {
                    format!("StepRange{{{elem}, {}}}", self.render_id(*step))
                } else {
                    format!("UnitRange{{{elem}}}")
                }
            }
        }
    }
}

/// Split `"Base{inner}"` into `("Base", "inner")`. Returns `None` when `name`
/// carries no well-formed `{…}` parameter list (retiring the ad-hoc
/// `parametric_type_args` `find('{')` + `ends_with('}')` gate, Issue #9197 S4).
fn split_parametric_type_name(name: &str) -> Option<(&str, &str)> {
    let open = name.find('{')?;
    if !name.ends_with('}') {
        return None;
    }
    Some((name[..open].trim(), &name[open + 1..name.len() - 1]))
}

/// Split a parameter list on **top-level** commas, ignoring commas nested inside
/// `{…}` or `(…)` (so `Tuple{Int64, String}` inside `Array{Tuple{Int64, String},
/// 1}`, and the `(:a, :b)` names tuple inside a `NamedTuple`, stay intact). The
/// returned slices are trimmed. This is the single structural top-level splitter
/// the S4 extractor uses in place of the retired `split_parametric_args` loop.
fn split_top_level_type_args(inner: &str) -> Vec<&str> {
    let mut args = Vec::new();
    let mut brace_depth = 0usize;
    let mut paren_depth = 0usize;
    let mut start = 0usize;
    for (idx, ch) in inner.char_indices() {
        match ch {
            '{' => brace_depth += 1,
            '}' => brace_depth = brace_depth.saturating_sub(1),
            '(' => paren_depth += 1,
            ')' => paren_depth = paren_depth.saturating_sub(1),
            ',' if brace_depth == 0 && paren_depth == 0 => {
                args.push(inner[start..idx].trim());
                start = idx + ch.len_utf8();
            }
            _ => {}
        }
    }
    let last = inner[start..].trim();
    // A trailing empty segment only occurs for a malformed/empty `"Foo{}"`; skip
    // it so `Foo{}` decomposes to a param-less struct rather than one empty child.
    if !last.is_empty() {
        args.push(last);
    }
    args
}

/// The sealed scalar/primitive type names that the S4 extractor classifies as a
/// [`ConcreteTypeKey::Primitive`] leaf (sealed ⇒ exact-name identity is correct,
/// mirroring `CALL_SITE_VALUE_PRIMITIVE_NAMES` in `vm/mod.rs`). Any other atomic
/// name is treated as a param-less nominal `Struct` — the classification only
/// changes the variant tag, never the rendered spelling.
fn is_sealed_primitive_name(name: &str) -> bool {
    matches!(
        name,
        "Int8"
            | "Int16"
            | "Int32"
            | "Int64"
            | "Int128"
            | "Int"
            | "UInt8"
            | "UInt16"
            | "UInt32"
            | "UInt64"
            | "UInt128"
            | "UInt"
            | "Float16"
            | "Float32"
            | "Float64"
            | "BigInt"
            | "BigFloat"
            | "Bool"
            | "Char"
            | "String"
            | "Symbol"
            | "Nothing"
            | "Missing"
            | "Any"
    )
}

/// Whether a range element name denotes a floating-point type, deriving the
/// `is_float` field of a [`ConcreteTypeKey::Range`] from its rendered element
/// (matches the `is_float` the L1 value path folds for `Value::Range`).
fn is_float_element_name(element: Option<&str>) -> bool {
    matches!(
        element.map(str::trim),
        Some("Float16" | "Float32" | "Float64" | "BigFloat")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The headline case from Issue #9197 / the `hash_struct_dispatch_identity`
    /// doc comment: `SubArray{Int64,1}` and `SubArray{Float64,2}` share a
    /// struct-table `type_id`, yet must be *distinct* concrete-type identities.
    /// The `N` (dimensionality) is a value type-parameter.
    #[test]
    fn param_distinct_subarray() {
        let mut t = TypeInternTable::new();

        let int64 = t.intern_primitive("Int64");
        let dim1 = t.intern(ConcreteTypeKey::IntValue(1));
        let subarray_i64_1 = t.intern_struct("SubArray", vec![int64, dim1]);

        let float64 = t.intern_primitive("Float64");
        let dim2 = t.intern(ConcreteTypeKey::IntValue(2));
        let subarray_f64_2 = t.intern_struct("SubArray", vec![float64, dim2]);

        // Distinct type parameters ⇒ distinct ids — the property `type_id` cannot
        // express.
        assert_ne!(subarray_i64_1, subarray_f64_2);
        assert_eq!(
            t.display_name(subarray_i64_1).unwrap(),
            "SubArray{Int64, 1}"
        );
        assert_eq!(
            t.display_name(subarray_f64_2).unwrap(),
            "SubArray{Float64, 2}"
        );

        // Re-interning the same spelling recovers the same id.
        let int64_again = t.intern_primitive("Int64");
        let dim1_again = t.intern(ConcreteTypeKey::IntValue(1));
        let subarray_i64_1_again = t.intern_struct("SubArray", vec![int64_again, dim1_again]);
        assert_eq!(subarray_i64_1, subarray_i64_1_again);
    }

    /// Re-interning any key is idempotent and does not grow the table.
    #[test]
    fn idempotent_reinterning() {
        let mut t = TypeInternTable::new();
        let a = t.intern_primitive("Int64");
        let len_after_first = t.len();
        let b = t.intern_primitive("Int64");
        assert_eq!(a, b);
        assert_eq!(t.len(), len_after_first);

        // A different key is a different id and grows the table by one.
        let c = t.intern_primitive("Float64");
        assert_ne!(a, c);
        assert_eq!(t.len(), len_after_first + 1);
    }

    /// Nested parametric types dedup by child id, and differ when a nested
    /// parameter differs.
    #[test]
    fn nested_parametric() {
        let mut t = TypeInternTable::new();

        let int64 = t.intern_primitive("Int64");
        let complex_int = t.intern_struct("Complex", vec![int64]);
        let arr_complex_int = t.intern(ConcreteTypeKey::Array {
            element: complex_int,
            ndims: 1,
        });

        let float64 = t.intern_primitive("Float64");
        let complex_float = t.intern_struct("Complex", vec![float64]);
        let arr_complex_float = t.intern(ConcreteTypeKey::Array {
            element: complex_float,
            ndims: 1,
        });

        assert_ne!(complex_int, complex_float);
        assert_ne!(arr_complex_int, arr_complex_float);
        assert_eq!(
            t.display_name(arr_complex_int).unwrap(),
            "Vector{Complex{Int64}}"
        );

        // Interning `Complex{Int64}` standalone recovers the id already used as
        // the array element (the DAG shares child nodes).
        let int64_again = t.intern_primitive("Int64");
        let complex_int_again = t.intern_struct("Complex", vec![int64_again]);
        assert_eq!(complex_int, complex_int_again);
    }

    /// Ids are assigned densely in first-seen order and stay stable as more
    /// types are interned; re-interning an early type returns its original id.
    #[test]
    fn id_stability_within_session() {
        let mut t = TypeInternTable::new();
        let first = t.intern_primitive("Int64");
        let second = t.intern_primitive("Float64");
        let third = t.intern(ConcreteTypeKey::Tuple(vec![first, second]));

        // Dense, monotonic ids in first-seen order.
        assert_eq!(first, ConcreteTypeId(0));
        assert_eq!(second, ConcreteTypeId(1));
        assert_eq!(third, ConcreteTypeId(2));

        // Interning many more types does not perturb earlier ids.
        for n in 0..100 {
            t.intern(ConcreteTypeKey::IntValue(n));
        }
        assert_eq!(t.intern_primitive("Int64"), first);
        assert_eq!(t.intern_primitive("Float64"), second);
        assert_eq!(
            t.key(first),
            Some(&ConcreteTypeKey::Primitive("Int64".into()))
        );
    }

    /// `display_name` renders canonical spellings for every key variant.
    #[test]
    fn display_round_trip() {
        let mut t = TypeInternTable::new();
        let int64 = t.intern_primitive("Int64");
        let float64 = t.intern_primitive("Float64");

        let tuple = t.intern(ConcreteTypeKey::Tuple(vec![int64, float64]));
        assert_eq!(t.display_name(tuple).unwrap(), "Tuple{Int64, Float64}");

        let nt = t.intern(ConcreteTypeKey::NamedTuple {
            names: vec!["a".into(), "b".into()],
            params: vec![int64, float64],
        });
        assert_eq!(
            t.display_name(nt).unwrap(),
            "NamedTuple{(:a, :b), Tuple{Int64, Float64}}"
        );

        let nt1 = t.intern(ConcreteTypeKey::NamedTuple {
            names: vec!["a".into()],
            params: vec![int64],
        });
        assert_eq!(
            t.display_name(nt1).unwrap(),
            "NamedTuple{(:a,), Tuple{Int64}}"
        );

        let matrix = t.intern(ConcreteTypeKey::Array {
            element: float64,
            ndims: 2,
        });
        assert_eq!(t.display_name(matrix).unwrap(), "Matrix{Float64}");

        let arr3 = t.intern(ConcreteTypeKey::Array {
            element: int64,
            ndims: 3,
        });
        assert_eq!(t.display_name(arr3).unwrap(), "Array{Int64, 3}");

        let mem = t.intern(ConcreteTypeKey::Memory { element: int64 });
        assert_eq!(t.display_name(mem).unwrap(), "Memory{Int64}");

        let unit_range = t.intern(ConcreteTypeKey::Range {
            element: int64,
            step: int64,
            is_float: false,
            is_step: false,
        });
        assert_eq!(t.display_name(unit_range).unwrap(), "UnitRange{Int64}");

        let step_range = t.intern(ConcreteTypeKey::Range {
            element: float64,
            step: float64,
            is_float: true,
            is_step: true,
        });
        assert_eq!(
            t.display_name(step_range).unwrap(),
            "StepRange{Float64, Float64}"
        );

        let color = t.intern(ConcreteTypeKey::Enum("Color".into()));
        assert_eq!(t.display_name(color).unwrap(), "Color");
    }

    /// `Range` ids distinguish the shape fields even when rendered spelling
    /// does not expose all of them, so a dispatch cache cannot conflate them.
    #[test]
    fn range_identity_tracks_all_shape_fields() {
        let mut t = TypeInternTable::new();
        let int64 = t.intern_primitive("Int64");
        let int8 = t.intern_primitive("Int8");
        let unit_int = t.intern(ConcreteTypeKey::Range {
            element: int64,
            step: int64,
            is_float: false,
            is_step: false,
        });
        let unit_int_float_flag = t.intern(ConcreteTypeKey::Range {
            element: int64,
            step: int64,
            is_float: true,
            is_step: false,
        });
        let step_int = t.intern(ConcreteTypeKey::Range {
            element: int64,
            step: int64,
            is_float: false,
            is_step: true,
        });
        let step_int8 = t.intern(ConcreteTypeKey::Range {
            element: int64,
            step: int8,
            is_float: false,
            is_step: true,
        });
        assert_ne!(unit_int, unit_int_float_flag);
        assert_ne!(unit_int, step_int);
        assert_ne!(step_int, step_int8);
    }

    /// `lookup` is `None` before interning and `Some(id)` afterward.
    #[test]
    fn lookup_before_and_after_intern() {
        let mut t = TypeInternTable::new();
        let key = ConcreteTypeKey::Primitive("Int64".into());
        assert_eq!(t.lookup(&key), None);
        let id = t.intern(key.clone());
        assert_eq!(t.lookup(&key), Some(id));
        assert!(!t.is_empty());
    }

    /// A faithful in-test copy of the **retired** runtime `parametric_type_args`
    /// (`vm/mod.rs`, deleted in Issue #9197 S4): the old string parse that
    /// extracted the top-level comma-separated type arguments from a rendered name
    /// (`find('{')` + `ends_with('}')` gate, then `{}`-depth-aware top-level comma
    /// split, dropping blanks). Kept here as the **oracle** the structured
    /// `intern_type_name` extractor is proven equivalent to.
    fn legacy_parametric_type_args(name: &str) -> Vec<String> {
        let Some(open) = name.find('{') else {
            return Vec::new();
        };
        if !name.ends_with('}') {
            return Vec::new();
        }
        let inner = &name[open + 1..name.len() - 1];
        let mut args = Vec::new();
        let mut depth = 0usize;
        let mut start = 0usize;
        for (idx, ch) in inner.char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => depth = depth.saturating_sub(1),
                ',' if depth == 0 => {
                    let seg = inner[start..idx].trim();
                    if !seg.is_empty() {
                        args.push(seg.to_string());
                    }
                    start = idx + ch.len_utf8();
                }
                _ => {}
            }
        }
        let last = inner[start..].trim();
        if !last.is_empty() {
            args.push(last.to_string());
        }
        args
    }

    /// Render the **top-level** structural parameters of an interned nominal
    /// `Struct` key back to strings — the structured analogue of the flat
    /// `legacy_parametric_type_args` list, used to prove decomposition equivalence.
    fn struct_top_level_params(t: &TypeInternTable, id: ConcreteTypeId) -> Vec<String> {
        match t.key(id) {
            Some(ConcreteTypeKey::Struct { params, .. }) => {
                params.iter().map(|&p| t.render_id(p)).collect()
            }
            other => panic!("expected a Struct key, got {other:?}"),
        }
    }

    /// Issue #9197 S4: the structured `intern_type_name` extractor produces, for a
    /// nominally-parametric name, **exactly** the top-level parameter list the
    /// retired `split_parametric_args` / `parametric_type_args` string parse did —
    /// now as interned child ids rather than re-parsed substrings. Covers the
    /// representative shapes the slice targets (`Dict{K,V}`, `SubArray`, nested
    /// parametrics); `Array{T,N}` / `Range` families are covered by the round-trip
    /// test below (their keys canonicalize `{T,1}`→`Vector`, `{T,T}`→one element).
    #[test]
    fn intern_type_name_top_level_params_match_legacy_split_issue_9197() {
        let mut t = TypeInternTable::new();

        for name in [
            "Dict{String, Int64}",
            "SubArray{Int64, 2}",
            "Foo{Bar{Int64}, Float64}",
            "Pair{Symbol, Vector{Int64}}",
            "Complex{Int64}",
        ] {
            let id = t.intern_type_name(name);
            assert_eq!(
                struct_top_level_params(&t, id),
                legacy_parametric_type_args(name),
                "structured top-level params must equal the retired parse for {name}"
            );
        }
    }

    /// Issue #9197 S4: decomposing a rendered name to structure and rendering it
    /// back is the round-trip proof that no information is lost relative to the
    /// type-name string. Covers `Array{T,N}` / `Vector` / `Matrix` / `Memory` /
    /// `UnitRange` / `StepRange` (whose keys compress the spelling) plus nested
    /// parametrics — the shapes the S4 unit-test contract names.
    #[test]
    fn intern_type_name_round_trips_representative_shapes_issue_9197() {
        let mut t = TypeInternTable::new();
        // (input spelling, canonical round-tripped spelling)
        let cases = [
            ("Array{Int64, 3}", "Array{Int64, 3}"),
            ("Array{Complex{Int64}, 1}", "Vector{Complex{Int64}}"),
            ("Vector{Float64}", "Vector{Float64}"),
            ("Matrix{Float64}", "Matrix{Float64}"),
            ("Memory{Int64}", "Memory{Int64}"),
            ("UnitRange{Int64}", "UnitRange{Int64}"),
            ("StepRange{Float64, Float64}", "StepRange{Float64, Float64}"),
            ("Dict{String, Int64}", "Dict{String, Int64}"),
            ("SubArray{Float64, 2}", "SubArray{Float64, 2}"),
            ("Tuple{Int64, Float64}", "Tuple{Int64, Float64}"),
            (
                "Dict{Symbol, Vector{Complex{Float64}}}",
                "Dict{Symbol, Vector{Complex{Float64}}}",
            ),
        ];
        for (input, expected) in cases {
            let id = t.intern_type_name(input);
            assert_eq!(
                t.display_name(id).unwrap(),
                expected,
                "round-trip of {input}"
            );
        }
    }

    /// Issue #9197 S4: the structured decomposition is **param-distinct** where the
    /// retired coarse key (`Struct { name: <full string>, params: [] }`) carried the
    /// parameters only inside the name — the `SubArray{Int64,1}` vs
    /// `SubArray{Float64,2}` headline — and it **shares** child ids across parents
    /// (the DAG dedups `Int64`). This is the behaviour-correcting improvement over
    /// the flat string parse: identity is now structural, by child id.
    #[test]
    fn intern_type_name_decomposes_params_structurally_issue_9197() {
        let mut t = TypeInternTable::new();

        let sub_i1 = t.intern_type_name("SubArray{Int64, 1}");
        let sub_f2 = t.intern_type_name("SubArray{Float64, 2}");
        assert_ne!(sub_i1, sub_f2, "distinct params ⇒ distinct ids");

        // The nominal base is decomposed, not embedded in the name string.
        match t.key(sub_i1) {
            Some(ConcreteTypeKey::Struct { name, params }) => {
                assert_eq!(&**name, "SubArray");
                assert_eq!(params.len(), 2);
            }
            other => panic!("expected structured SubArray, got {other:?}"),
        }

        // The `Int64` child id is shared (deduped) between `SubArray{Int64,1}` and
        // `Vector{Int64}` — the registry is a DAG, not a tree of re-parsed names.
        let int64_standalone = t.intern_type_name("Int64");
        let vec_int = t.intern_type_name("Vector{Int64}");
        let (
            Some(ConcreteTypeKey::Struct { params, .. }),
            Some(ConcreteTypeKey::Array { element, .. }),
        ) = (t.key(sub_i1), t.key(vec_int))
        else {
            panic!("unexpected keys");
        };
        assert_eq!(
            params[0], int64_standalone,
            "SubArray's Int64 child is the interned Int64"
        );
        assert_eq!(
            *element, int64_standalone,
            "Vector's Int64 element is the same id"
        );
    }

    /// Issue #9427: the `Opaque` key (re-cached type-object / function-singleton /
    /// module / … dispatch identities) renders by its stored name, distinct names
    /// stay distinct, and — being a distinct variant — an `Opaque("Foo")` never
    /// collides with a same-spelling `Primitive`/`Enum`/`Struct` key. That
    /// variant-tag separation is what makes re-caching these kinds sound.
    #[test]
    fn opaque_key_renders_and_never_collides_across_variants_issue_9427() {
        let mut t = TypeInternTable::new();

        let type_int = t.intern(ConcreteTypeKey::Opaque("Type{Int64}".into()));
        let type_float = t.intern(ConcreteTypeKey::Opaque("Type{Float64}".into()));
        assert_ne!(type_int, type_float, "distinct opaque names ⇒ distinct ids");
        assert_eq!(t.display_name(type_int).unwrap(), "Type{Int64}");

        // Idempotent: re-interning the same opaque name returns the same id.
        assert_eq!(
            type_int,
            t.intern(ConcreteTypeKey::Opaque("Type{Int64}".into()))
        );

        // A same-spelling Primitive / Enum / Struct key is a DIFFERENT variant ⇒
        // a different id (no cross-variant conflation).
        let prim_foo = t.intern(ConcreteTypeKey::Primitive("Foo".into()));
        let enum_foo = t.intern(ConcreteTypeKey::Enum("Foo".into()));
        let struct_foo = t.intern_struct("Foo", Vec::new());
        let opaque_foo = t.intern(ConcreteTypeKey::Opaque("Foo".into()));
        let ids = [prim_foo, enum_foo, struct_foo, opaque_foo];
        for i in 0..ids.len() {
            for j in (i + 1)..ids.len() {
                assert_ne!(ids[i], ids[j], "variant-tagged keys must not collide");
            }
        }
    }
}
