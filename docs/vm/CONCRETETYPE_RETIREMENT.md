# Retiring `ConcreteType`: the `CoreType` + lattice-only-carriers wrapper (Issue #6720, Phase 6)

*Created: 2026-06-17*

Design for the remaining body of Issue #6720 / epic #5916 §4 Phase 4–6: giving
the abstract-interpretation lattice a `CoreType`-based concrete payload. This is
the companion plan to [TYPE_REPRESENTATIONS.md](./TYPE_REPRESENTATIONS.md); read
its §1.3 / §4 "Phases" first.

## 1. Decision: wrapper, not "enrich `CoreType`"

The literal framing — "give `CoreType` `type_id` / `captures` / `Enum`" — was
**rejected after investigation**. `CoreType` is the shared semantic core:

- **1706** `CoreType::` uses; exhaustively matched by the subtype engine
  (`type_core/intersect.rs`, `type_core/match.rs`, `specificity.rs`,
  `dispatch_resolver/`).
- It is the **single serialized source of truth** for method signatures
  (`MethodSig.core_signature`, Issue #6336). Adding fields/variants changes the
  serialized format → cache-version bump + MethodSig round-trip risk.
- `type_id` is a **compiler-struct-table index**, not a Julia-type property; it
  is already resolvable from the struct *name* (`abstract_interp/struct_info.rs`,
  "by looking up the struct name") and many sites use `type_id: 0` as a
  "resolve later" sentinel.

Putting VM/codegen artifacts into the semantic core would force the subtype
engine to carry meaningless arms and would mutate the serialized signature
format. **Decision (confirmed 2026-06-17): keep `CoreType` pure** and make the
lattice's concrete payload a thin wrapper.

### Status (2026-06-19): nullary fold landed, carrier scope finalized

The **nullary half is done** — all primitive / abstract / `Any` variants are now
`Core(CoreType)` (PRs #6901 abstracts, #6902 primitives + `Any`; smart
constructors + the #6817 hazard in #6900). The realized shape:

```rust
// realized shape of ConcreteType (compile/lattice/types.rs) — Issue #6720
enum ConcreteType {
    /// All nullary types (primitives / abstracts / `Any`) delegate to the
    /// shared, structurally-complete core. (Folded — PRs #6901, #6902.)
    Core(CoreType),

    // ── lattice-only carriers ──
    // (a) function-like / dispatch markers CoreType can't represent faithfully:
    Function { name: String },
    Closure { name: String, captures: Vec<(String, ConcreteType)> },
    ComposedFunction { outer: Box<ConcreteType>, inner: Box<ConcreteType> },
    Enum { name: String },                                       // #2863
    // (b) structured containers whose CHILDREN are `ConcreteType` — see §2.2:
    Array { element: Box<ConcreteType>, ndims: Option<usize> },  // + #6817 rank
    Tuple { elements: Vec<ConcreteType> },
    TupleVararg { elements: Vec<ConcreteType>, tail: Box<ConcreteType> },
    NamedTuple { fields: Vec<(String, ConcreteType)> },
    Range { element: Box<ConcreteType> },
    Dict { key: Box<ConcreteType>, value: Box<ConcreteType> },
    Set { element: Box<ConcreteType> },
    Generator { element: Box<ConcreteType> },
    UnionOf(Vec<ConcreteType>),
    // (c) name-only / Named-nullary — foldable but deferred (low value), §2.3:
    Struct { name: String, type_id: usize },
    DataType { name: String }, Module { name: String },
    Pairs, Expr, QuoteNode, LineNumberNode, GlobalRef, Regex, RegexMatch,
}
```

This realizes the TYPE_REPRESENTATIONS.md end-state ("`ConcreteType` becomes a
thin wrapper over `CoreType` + lattice-only carriers") — the carrier set is
simply larger than first anticipated, because the structured containers are
*inherently* lattice-only (§2.2). The **representation flip is practically
complete**: the dual nullary representation is gone; what remains (§2.3) is
optional polish with marginal benefit.

## 2. Variant inventory (ground truth from `From<&ConcreteType> for CoreType`)

Source: `compile/lattice/types.rs` (`impl From<&ConcreteType> for CoreType`).

### 2.1 Faithful → fold into `Core(CoreType)`

These already convert to `CoreType` **without losing load-bearing info**, so
they become `Core(..)` with no behavioural change:

| `ConcreteType` | `CoreType` image |
|---|---|
| `Int8…BigFloat`, `Bool`, `String`, `Char`, `Symbol`, `Nothing`, `Missing` | `Primitive(_)` |
| `Any` | `Any` |
| `Number` / `Integer` / `AbstractFloat` | `Abstract(_)` |
| `Array{el}` / `Dict{k,v}` / `Set{el}` / `Generator{el}` | `Struct{name, params}` |
| `Range{el}` | `Abstract(AbstractRange)` (el=Any) / `Struct{"AbstractRange",[el]}` |
| `Tuple{els}` / `TupleVararg{els,tail}` | `Tuple(_)` (+ trailing `Vararg`) |
| `NamedTuple{fields}` | `NamedTuple(_)` |
| `UnionOf(types)` | `Union(_)` |
| `Struct{name, type_id}` | `from_julia_name(name)` — **`type_id` dropped** (see §3) |
| `DataType{name}` | `TypeOf(from_julia_name(name))` — name **preserved** |
| `Module{name}` | `Module(name)` — name **preserved** |
| `Pairs`, `Expr`, `QuoteNode`, `LineNumberNode`, `GlobalRef`, `Regex`, `RegexMatch` | `Named(_)` |
| `IO` | `Abstract(IO)` |

### 2.2 Lattice-only carriers (CoreType loses load-bearing info)

| `ConcreteType` | current `CoreType` image | what is lost | keep as |
|---|---|---|---|
| `Function{name}` | `Abstract(Function)` | the **name** | carrier `Function{name}` |
| `Closure{name,captures}` | `Abstract(Function)` | name **+ captures** (codegen) | carrier `Closure{..}` |
| `ComposedFunction{outer,inner}` | `Abstract(Function)` | the **structure** | carrier `ComposedFunction{..}` |
| `Enum{name}` | `Named(name)` | the **enum-ness** (dispatch #2863) | carrier `Enum{name}` |
| `Array{element, ndims: Some(n)}` | `Struct{"Array",[element]}` | the **rank `n`** (dispatch #6817) | see note below |

`Struct.type_id` is *not* a carrier variant — it is an attachment resolvable
from the name (§3).

**Structured containers stay carriers (decided 2026-06-19, after measurement).**
§2.1 originally listed the containers (`Tuple` / `TupleVararg` / `NamedTuple` /
`Range` / `Dict` / `Set` / `Generator` / `UnionOf`, plus `Array`) as "faithful →
fold to `Core`". In practice they must **remain carriers**, for the same reason
`Array` does: their **children are `ConcreteType`**, and the lattice operations
(`join` / `widen` / `subtype` / `type_depth` / element extraction) pattern-match
those children directly — e.g. `ConcreteType::Tuple { elements }` is bound at
**38** sites, `Dict { key, value }` at **14**. Folding them to
`Core(CoreType::Tuple(Vec<CoreType>))` etc. would change the children to
`CoreType`, forcing a `CoreType ↔ ConcreteType` conversion at every one of those
recursive sites — added per-access cost + verbosity for **no** semantic gain
(`From<&ConcreteType> for CoreType` already projects them losslessly when a
`CoreType` view is actually needed). So the carriers are: the function-like set
(`Function`/`Closure`/`ComposedFunction`/`Enum`) **and** the structured
containers (`Array`/`Tuple`/`TupleVararg`/`NamedTuple`/`Range`/`Dict`/`Set`/
`Generator`/`UnionOf`). This is the realized §1 shape and the practical
completion of the flip.

### 2.3 Name-only / `Named`-nullary — foldable but DEFERRED (low value)

`Struct{name,type_id}`, `DataType{name}`, `Module{name}`, and the `Named`-mapped
nullaries (`Pairs`/`Expr`/`QuoteNode`/`LineNumberNode`/`GlobalRef`/`Regex`/
`RegexMatch`) carry **no `ConcreteType` children**, so they *could* fold to
`Core(Named(..))` / `Core(TypeOf(..))` / `Core(Struct{name,[]})`. Deferred because
the benefit is marginal and each costs per-site pattern restructuring: the
`Named`-mapped ones need `Core(CoreType::Named(n)) if n == "Expr"` guards
(a `String` can't appear in a plain pattern), and `Struct` additionally needs the
§3 `type_id`-reader rework. Left as optional future polish; not required for the
flip to be "done".

**Array rank correction (#6817, found 2026-06-18 during Commit A).** §2.1 lists
`Array{el}` as faithful, but that holds **only for `ndims: None`**.
`ConcreteType::Array.ndims` reaches `Some(n)` via `bridge.rs:52`
(`ValueType::ArrayOf(_, Some(n))`, e.g. a 2-D `Matrix` comprehension), and
`From<&ConcreteType> for CoreType` (`types.rs:668`, `Array { element, .. }`)
**drops the rank**; the read sites `bridge.rs:1995 / :2086` consume it for
`::Matrix` vs `::Vector` dispatch. So naively folding `Array` into
`Core(Struct{"Array",[el]})` would regress #6817, and the Slice-1 classification
test does **not** catch it (it only pins `ndims: None`). Commit B must either
(a) keep `Array{element, ndims}` as a lattice-only **carrier** (lower risk,
preferred), or (b) encode the rank in the core image as
`Struct{"Array",[el, CoreType::Value(n)]}` (faithful, but needs a `From`-impl
change on both directions + subtype-engine verification). Extend the Slice-1 pin
with an `Array{ndims: Some(2)}` case before flipping.

## 3. `type_id` resolution strategy

`ConcreteType::Struct{name, type_id}` carries a struct-table index read at a few
sites (field-offset get/set #5085, reflection). In the wrapper, the struct lands
as `Core(CoreType::Struct{name, params})` with **no** `type_id`.

- **Read sites** resolve `name → type_id` via the compiler struct table
  (`SharedCompileContext::get_struct_type_id` / `struct_info.rs`), which already
  exists and is the source of truth. The `type_id: 0` "unresolved" sentinel
  disappears (the name is always present and authoritative).

### 3.1 `type_id` reader audit (Slice 1, completed 2026-06-17)

Sites that **read** `ConcreteType::Struct.type_id` (bind the value, not `..`),
excluding constructors and tests. Only **three** production readers exist:

| reader | what it does with `type_id` | table in scope? | Slice-2 plan |
|---|---|---|---|
| `expr_tfuncs.rs` `value_array_element_from_concrete` | `*type_id != 0 → StructOf(*type_id)` | ✅ **yes** — `struct_type_id: FnMut(&str) -> Option<usize>` already a parameter | resolve `struct_type_id(name)` (the closure is already passed) |
| `expr_tfuncs.rs` `constructor_lattice_to_value_type` | `→ ValueType::Struct(*type_id)` | ⚠️ caller `infer_value_complex_call`/`:523` builds a `StructIdLookup` (`with_struct_ids`) | thread the lookup into this helper (1-arg signature change) |
| `bridge.rs` `convert_concrete_to_array_element` (`:791`) | Complex placeholders by **name**; else `StructOf(id)` | ❌ **no** table param (caller `:334`) | the `Complex{..}` arms already key off the name; thread a `name → id` resolver into the fn + its caller for the generic `StructOf` arm |

`bridge.rs:1056` is a **test**, not a production reader. All constructors
(`bridge.rs:78/488/492/688`, `complex_ops.rs:251`, `pipeline_ctx.rs:324`, …)
simply *stop passing* `type_id` after the flip and are not blockers.

**Conclusion:** the `type_id` resolution risk is **bounded and tractable** — two
of the three readers already have (or trivially receive) a name→id resolver; only
`convert_concrete_to_array_element` needs a resolver threaded through one caller
hop. No reader is a hard blocker.

## 4. Hazards to preserve (inherited from #5916 / #6599 / CLAUDE.md)

- **`CoreType` stays byte-identical** → subtype engine, `typeintersect`,
  `typejoin`, specificity, and `MethodSig.core_signature` serialization are
  **untouched** (no cache-version bump from this work).
- **Struct field offsets (#5085)**: `type_id` must resolve correctly at every
  read site (§3).
- **`Eq`/`Hash` identity change**: today `Struct{name, type_id: 0}` and
  `Struct{name, type_id: 5}` compare **unequal**; after the flip both become
  `Core(Struct{name, params})` and compare **equal** (`CoreType::Struct` carries
  no `type_id`). Audit every site that uses `ConcreteType` as a cache key, in
  `UnionOf` dedup, or in env-merge — the `engine/tests/cache_invalidation.rs`
  suite should surface any behavioural drift. The `type_id: 0` "unresolved"
  sentinel disappears, so each §3.1 reader's name→id resolver must reproduce the
  current `type_id: 0` fallback when the struct table has no entry.
- **Enum dispatch (#2863)**: the `Enum{name}` marker is retained, not folded to
  `Named`.
- **Closure specialization**: `captures` retained.
- **`LatticeType::Bottom → ValueType::Any` widen** and the **#4679** `Union{}`
  recovery special-case are in `LatticeType`/bridge, *not* `ConcreteType`, and
  are out of scope here — must remain untouched.
- **Promote-fallback recursion (#5966)**: dispatch-order/cache/seed dependent;
  after each slice run the **full** `cargo nextest run --release` (never `| tail`
  the failure away) plus `scripts/test_aot.sh`.

## 5. Migration slices (multi-PR)

Each slice keeps the tree compiling and the full suite + AoT gate green.

0. **Design** (this document). ✅
1. **Classification pin + `type_id` reader audit** (additive, no representation
   change). ✅ **Done 2026-06-17:** a characterization test
   (`concretetype_coretype_roundtrip_classification_issue_6720`,
   `compile/lattice/types.rs`) pins *which* `ConcreteType` variants are
   `CoreType`-faithful vs lattice-only (golden snapshot of the round-trip, so
   later slices cannot silently change §2); the `type_id` reader audit (§3.1)
   confirms only three production readers, none a hard blocker. Behaviour-preserving.
   (The `Core(..)` constructor/accessor helpers were deferred to Slice 2, where
   they are actually used together with the name→`type_id` resolver — adding them
   now would be unused/dead.)
2. **Representation flip — nullary fold (DONE 2026-06-18/19).** The original
   "one big PR" plan was abandoned: a whole-enum flip in one shot is unmanageable
   — the import bookkeeping for `CoreType`/`CorePrimitive`/`CoreAbstract` across
   60+ files (lib vs `#[cfg(test)]` vs both) cascaded and was reverted. Instead
   the flip landed as **green sub-batches sized by FILE COUNT**:
   - **#6900** — Commit A: behaviour-preserving smart constructors + the #6817
     hazard finding (§2.2).
   - **#6901** — fold the abstracts (`Number`/`Integer`/`AbstractFloat`/`IO`, 9 files).
   - **#6902** — fold the 21 primitives + `Any` (61 files).

   Per-slice procedure: perl global replace `ConcreteType::X` →
   `ConcreteType::Core(CoreType::Primitive(CorePrimitive::X))` (valid in BOTH
   value and pattern, and through `&ConcreteType` patterns — unlike associated
   `const`s, which fail `&`-patterns with a type mismatch); enum surgery; collapse
   the folded `From<&ConcreteType> for CoreType` arms to `Core(c) => c.clone()`;
   add `Core(_) =>` arms to the few exhaustive conversion matches (bridge
   `From<&ConcreteType> for ValueType` / `convert_concrete_to_array_element`,
   `julia_type_from_concrete_type`, `type_depth`); place imports **per scope**
   (file-top for lib use, `#[cfg(test)]`-gated or inside `mod tests` for test-only
   use), guided by `clippy --all-targets -- -D warnings`. NB: `cargo fix --lib`
   wrongly strips test-only imports → never use it; detect existing imports with
   `perl -0777` (line-grep misses multi-line `use {..}`). Also un-leaked the
   `Core(..)` Debug from the type-stability report (`format_lattice_type` →
   `to_type_name`, caught by `type_stability_uses_global_types_for_const_reader`).
3. **Carrier-scope finalization (DONE 2026-06-19).** The structured containers
   are kept as carriers, not folded (§2.2) — they hold `ConcreteType` children
   the lattice recurses on, so folding them buys nothing and adds per-access
   conversions. With this the dual nullary representation is gone and the flip is
   **practically complete**.
4. **(Optional, deferred)** the name-only / `Named`-nullary fold (§2.3) and any
   `LatticeType::core()` ergonomics — marginal benefit, per-site cost; not
   required for the flip to be "done".

## 6. Verification (per slice)

- `timeout 1800 cargo nextest run --release` (full workspace — never tail).
- `bash scripts/test_aot.sh` (AoT gate).
- `cargo clippy --all-targets -- -D warnings`.
- Behaviour-preservation pins: `test_*_issue_5916`, `test_*_issue_6599`, the
  Slice-1 faithful/carrier classification test, and `bridge` round-trips.
