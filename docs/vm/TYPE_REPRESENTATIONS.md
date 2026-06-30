# Type Representations: Complete Conversion Inventory (Issue #5916)

*Last updated: 2026-06-16*

This is the detailed companion to the summary table in
[TYPE_SYSTEM.md](./TYPE_SYSTEM.md#type-representation-conversion-inventory-issue-5916).
It maps **every** type representation in the codebase, **every** conversion
between them (with `file:line` and what each conversion loses), and derives the
consolidation roadmap for Issue #5916.

All paths below are relative to `subset_julia_vm/src/`.

---

## 1. The representations

### 1.1 `JuliaType` — canonical runtime type (user-facing)

- **Where**: `types/julia_type/mod.rs:73` (~1066 lines in `mod.rs`; methods split across
  `parsing.rs`, `display.rs`, `comparison.rs`, …)
- **Owner**: `types/` (shared by lowering, compiler, VM, reflection)
- **Role**: the type spelling used by `typeof()`, method signatures
  (`MethodSig.params` in-memory projection), struct field types, error messages.
- **Unique info**: dedicated variants for VM-special shapes (`VectorOf`,
  `MatrixOf`, `TupleOf`, `Enum`, `Expr`/`QuoteNode`/`LineNumberNode`/`GlobalRef`,
  `IOBuffer`, `Pairs`, `Generator`).
- **Structural weakness**: parametric user types are an **opaque string** —
  `JuliaType::Struct("Complex{Float64}")`. Any conversion involving parameters
  of a user struct therefore degrades to string rendering/parsing. `TypeVar`
  carries only an *upper* bound, and as a `String`, not a structured type.

### 1.2 `CoreType` — shared structured semantic core

- **Where**: `inference_core/type_core/repr.rs:182` (enum) + facade
  `inference_core/type_core.rs` (~5071 lines: subtype engine, intersect, join,
  specificity); split by Issue #5917.
- **Owner**: `inference_core/`
- **Role**: substrate of `CoreSubtypeEngine`, `typeintersect`, `typejoin`,
  specificity, and (since Issue #6336) the **single serialized source of truth**
  for method signatures (`MethodSig.core_signature`).
- **Unique info**: the only representation with **fully structured** parametric
  types (`Struct { name, params }`), typevars with *both* bounds
  (`CoreTypeVar { lower_bound, upper_bound }` as structured types), `UnionAll`,
  `Vararg` / `VarargLen`, `NamedTuple`, and bits **value parameters**
  (`CoreValueParam` for `Val{1}`, `Array{T,2}`).
- **Escape hatch**: `CoreType::Named(String)` — fallback for names the bridge
  cannot structure yet.

### 1.3 `LatticeType` + `ConcreteType` + `ConstValue` — inference lattice

- **Where**: `compile/lattice/types.rs:94` (`LatticeType`), `:163`
  (`ConcreteType`), `:29` (`ConstValue`)
- **Owner**: `compile/abstract_interp/` + `compile/tfuncs/`
- **Role**: abstract-interpretation lattice
  (`Bottom < Const < Concrete < Union < Conditional < Top`).
- **Unique info**: constants (`Const(42)`), flow-sensitive `Conditional`
  (slot + then/else types), `TupleVararg` normalization (Issue #3511),
  `Struct { name, type_id }` (the `type_id` links to the compiler struct table),
  `Closure { captures }`.
- **Structural weakness**: like `JuliaType`, parametric struct params live
  *inside the name string* (`ConcreteType::Struct { name: "Complex{Float64}" }`).

### 1.4 `ValueType` — VM runtime type tag

- **Where**: `vm/value/value_enum.rs:854` (+ `ArrayElementType` at
  `vm/value/array_element.rs:10`)
- **Owner**: `vm/`
- **Role**: codegen instruction selection, slot typing, dynamic dispatch tags.
  Deliberately coarse (e.g. one `Tuple`, `Struct(type_id)` with no name).
- **Unique info**: the VM carrier reality — `ComplexF32/ComplexF64` unboxed
  fast-path tags, `ArrayOf(ArrayElementType)` / `MemoryOf(..)` storage layout.
- **Structural weakness** (resolved, Issue #6720): `ArrayElementType::UnionOf`
  formerly embedded a textual `Union{...}` body string that
  `compile/bridge.rs` had to re-parse — a string-typed channel inside an
  otherwise structural enum. It now carries structured members
  (`UnionOf(Vec<JuliaType>)`); the bridge and `array_element_type_to_julia_type`
  canonicalize the members directly via `canonicalize_union` (no string
  round-trip), and `ArrayElementType::union_from_body` lifts a body string into
  members only at genuine string-source boundaries.

### 1.5 `TypeExpr` — lowering-side type AST

- **Where**: `types/type_expr.rs:18`
- **Owner**: lowering (struct field declarations, `Dict{K,V}()` literals,
  constructor type args)
- **Role**: *unresolved* type expressions that may reference struct type
  parameters: `Concrete(JuliaType) | TypeVar(String) | Parameterized { base,
  params } | RuntimeExpr(String)`.
- **Unique info**: `RuntimeExpr` (source text evaluated at runtime, e.g.
  `MIME{Symbol(s)}`) — nothing else can represent this.

### 1.6 AoT projections: `aot::JuliaType` and `aot::StaticType`

- **Where**: `aot/types.rs:20` (`JuliaType`), `aot/types.rs:215` (`StaticType`)
- **Owner**: `aot/` (feature-gated; intentional projections)
- **Role**: `StaticType` = Rust-codegen layout types (`I64`, `Str`,
  `Array { element, ndims }`, `Function { params, ret }`); `aot::JuliaType` =
  AoT-side Julia syntax projection used by the AoT inference engine/codegen.
- Both bridge to the shared taxonomy through `CoreType`
  (`primitive_numeric()` at `aot/types.rs:106` / `:377`).

### Auxiliary (annotations, not full representations)

| Type | Where | Note |
|---|---|---|
| `TypeParam` | `types/type_param.rs` | `where`-clause param: name + **string** bounds (+ legacy `bound` mirror of `upper_bound`) |
| `CoreTypeVar` | `type_core/repr.rs:143` | structured counterpart of `TypeParam` |
| `VarTypeTag` | `vm/slot.rs` | tiny slot-typing projection of `ValueType` (`value_type_to_var_type_tag`, `vm/slot.rs:437`) |
| `PrimitiveNumeric` | `inference_core/primitive_numeric.rs` | shared numeric taxonomy; reached *via* `CoreType` |
| type-name `String` | everywhere | the de-facto 7th representation — see §4 |

---

## 2. The conversion graph

```
                          TypeExpr ──(string render + re-parse, §4.1)──▶ JuliaType
                              │                                            │  ▲
                              │ type_expr_to_static (AoT)                  │  │ value_type_to_julia_type ×3 (§5.2)
                              ▼                                            ▼  │
   aot::StaticType ◀──From── vm JuliaType ──From──▶ CoreType ◀──From── LatticeType/ConcreteType
        │   ▲                     ▲                  │   ▲                 ▲   │
        │   └── from_core_type_lossy                 │   │                 │   │
        └──────From──────▶ CoreType ◀────From──── aot::JuliaType           │   │
                                                                           │   │
                     core_type_to_julia_type (§3 B)                        │   │
   JuliaType ◀───────────────────────────────────── CoreType              │   │
        │                                                                  │   │
        │ julia_type_to_value_type ×4 (§5.1)         julia_type_to_lattice (canonical, §3.6)
        ▼                                                                  │   │
    ValueType ◀────────── From / lattice_to_value_type ────────────────────┘   │
        │                                                                      │
        └────────────── From / value_type_to_lattice_with_struct_table ────────┘
```

`CoreType` is already the hub for *semantic* questions (subtype/join/intersect),
but `JuliaType ↔ ValueType ↔ LatticeType` conversions still form a triangle of
direct edges that bypass it, each re-implemented several times (§5).

### Full conversion table

Legend: **L** = lossy (information dropped/defaulted), **S** = goes through a
string, **D** = duplicated implementation exists.

| # | From → To | Entry point | file:line | Flags | What is lost / defaulted |
|---|---|---|---|---|---|
| 1 | `JuliaType` → `CoreType` | `impl From<&JuliaType>` | `inference_core/type_core/convert.rs:3` | L,S | TypeVar/UnionAll **bound strings** re-parsed via `from_julia_name`; bare `Array`/`Dict`/`Set` have no params to carry; **non-injective** (`JT::Pairs` vs `JT::Struct("Pairs")` share one image) |
| 2 | `CoreType` → `JuliaType` | `core_type_to_julia_type` | `type_core/convert.rs:248` | L,S | TypeVar **lower bounds** dropped (`JT::TypeVar` has none); `Named`/`Vararg`/`NamedTuple`/value-params collapse to `JT::Struct(to_julia_name())` strings. Round-trip for lowering-produced spellings pinned by `base_method_params_roundtrip_core_signature_issue_6336` |
| 3 | `TypeParam` → `CoreTypeVar` | `impl From<&TypeParam>` | `type_core/convert.rs:122` | S | bound *strings* parsed (`core_upper/lower_bound_from_name`) |
| 4 | `CoreTypeVar` → `TypeParam` | `core_type_var_to_type_param` | `type_core/convert.rs:343` | S | structured bounds re-rendered to strings (+ legacy `bound` mirror) |
| 5 | `aot::JuliaType` → `CoreType` | `impl From` (cfg aot) | `type_core/convert.rs:138` | L | `Unknown` → `Any` |
| 6 | `aot::StaticType` → `CoreType` | `impl From` (cfg aot) | `type_core/convert.rs:190` | L | `Function` signature dropped → `Abstract(Function)`; `Array` ndims dropped; struct `type_id` dropped |
| 7 | `JuliaType` → `aot::StaticType` | `impl From<&JuliaType>` | `aot/types.rs:819` | L | **`from_vm_julia_type_lossy` (#8, CoreType-routed) runs first**; the manual fallback now only owns the shapes CoreType deliberately leaves unprojectable: `BigInt`/`BigFloat` → `Any`; bare `Tuple`/`Dict`/`Set` shells; `UnitRange`/`StepRange` → `Range{I64}`; user `Struct` → `Struct{0,name}`; `Enum` → `I32`; abstract families/`Symbol`/`TypeVar`/`Bottom`/`TypeOf` → `Any`; `Union` all-`Any` collapse; `UnionAll` unwrapped to body. The redundant `Array`/`MatrixOf` arms were **removed** in #6598 (the `Vector`/`Matrix`/`Array` family already projects identically through CoreType — pinned by `aot::types::tests::test_issue_6598_array_projections_route_through_core_type`) |
| 8 | `JuliaType` → `Option<StaticType>` | `from_vm_julia_type_lossy` | `aot/types.rs:489` | L | routed via `CoreType` then `from_core_type_lossy` (the modern path; #7 falls back to it first) |
| 9 | ~~`JuliaType` → `aot::JuliaType`~~ | ~~`impl From<&JuliaType>`~~ | ~~`aot/types.rs:889`~~ | **DEAD** | **zero external call sites (only self-recursion) — removed by this PR** |
| 10 | `ValueType` → `LatticeType` | `impl From<&ValueType>` | `compile/bridge.rs:20` | L | `Struct(type_id)` gets synthetic name `"Struct#N"` (no table); `Any` → `Top`; **empty `Union` → `Bottom`** (Issue #5916; non-empty unions whose arms all fail to convert still widen to `Top`) |
| 11 | `ValueType` → `LatticeType` (table) | `value_type_to_lattice_with_struct_table` | `compile/bridge.rs:196` | L | fixes #10's struct names + `ComplexF32/64`; non-`Concrete` union arms are silently dropped; empty `Union` → `Bottom` (Issue #5916) |
| 12 | `ValueType` → `LatticeType` (dup) | `value_type_to_lattice` / `_with_table` | `compile/abstract_interp/struct_info.rs:141,153` | D | parallel impl of #10/#11 for struct field info; still maps empty `Union` to `LatticeType::Union(∅)` (not `Bottom`) — residual divergence, owned by the #5922 track |
| 13 | `LatticeType` → `ValueType` | `impl From<&LatticeType>` + wrapper `lattice_to_value_type` | `compile/bridge.rs:262`, `:1315` | L | `Const` value dropped; `Conditional` widened; `Bottom` → `Any` (**deliberate**, documented widening since Issue #5916 — see §3.5; the exact empty-union carrier was tried and reverted); `Function`/`Closure` → `Any` |
| 14 | `LatticeType` → `Option<JuliaType>` | `lattice_to_parametric_julia_type` | `compile/bridge.rs:1324` | S | exists *because* #13 loses parameters; preserves `Bottom` → `JT::Bottom` (Issue #4679 — stays necessary, see §3.5: the `ValueType` fallback deliberately widens Bottom); Dict spelled as a **string** `"Dict{K,V}"`. **Issue #6599**: the structure-preserving (`#14`) vs string-opaque (`#15`) divergence on a braced `ConcreteType::Struct { name }` is **unified** — both now parse the braced spelling through `from_name_or_struct` (pinned by `bridge::test_lattice_to_julia_type_pair_agrees_on_braced_struct_issue_6599`) |
| 15 | `LatticeType` → `JuliaType` | `lattice_to_julia_type` | `compile/bridge.rs:1373` | L | total version of #14; `Top`/`Conditional` → `Any`; `Bottom` → `JT::Bottom` (Issue #5916). **Issue #6599**: now agrees with #14 on braced parametric struct spellings (via the shared `concrete_type_to_julia_type`, #16) — `Vector{Int64}` → `JT::VectorOf(Int64)`, not the opaque `JT::Struct("Vector{Int64}")` (upstream-verified: `Vector{Int64} === Array{Int64,1}` is a concrete parametric `DataType`). Bare (non-braced) names keep their opaque `JT::Struct(name)` form — the conservative gate that prevents a struct literally named `Int64`/`ComplexF32` from being reinterpreted as the primitive/alias |
| 16 | `ConcreteType` → `JuliaType` | `concrete_type_to_julia_type` (private) | `compile/bridge.rs:1281` | L,S,D | struct params stay inside name strings **except** braced parametric spellings, which Issue #6599 routes through `from_name_or_struct` (so `Vector{Int64}`/`Matrix{T}`/`Complex{Float64}` recover their structured `JuliaType`); this is the shared core of #14/#15 |
| 17 | `ConcreteType` → `JuliaType` (dup) | `concrete_type_to_julia` | `compile/abstract_interp/engine/mod.rs:6420` | D | parallel impl of #16 |
| 18 | `JuliaType` → `ConcreteType` | **canonical** `julia_type_to_concrete_type_lossy` (now `pub(crate)`) | `compile/bridge.rs` | L | abstract families collapse; sibling-reachable since Issue #5916 wave 3; bare `Array` → `Array{Any}` since wave 4 (previously fell through to `Any`, drifting from every lattice copy) |
| 19 | `JuliaType` → `ConcreteType` (lattice projection) | `julia_type_to_concrete_or_any_with_struct_resolver` (`pub(crate)`) | `compile/bridge.rs` (Issue #5916 wave 4) | — | canonical projection of #20 used for element/parameter positions; the two sibling copies (`analyzer.rs`, `engine/mod.rs`) were **deleted** in wave 4 when their hosts' `julia_type_to_lattice` started delegating (§3.6) |
| 20 | `JuliaType` → `LatticeType` | **canonical** `julia_type_to_lattice` / `_with_struct_table` / `_with_struct_resolver` | `compile/bridge.rs` (Issue #5916 waves 3–5) | — | single in-scope source of truth; all 4 sibling-owned copies now **delegate** (`type_stability/analyzer.rs`, `abstract_interp/engine/mod.rs` via struct-id resolver, `vm/builtins_reflection/mod.rs`, and since wave 5 `expr/infer/expr_tfuncs.rs` — the tfunc-argument edge keeps explicitly pinned adapter divergences: deferral edges → `Top`, `TypeOf → DataType{name}`, legacy range/abstract pinnings). **Issue #6600**: the `TupleOf → Tuple{}` pin was **removed** (now delegated; element types never surface at the adapter level); an adapter-level audit (`pin_audit_load_bearing_arms_diverge_dead_arms_match`) proved every remaining pin is load-bearing — see §3.6 |
| 21 | `LatticeType` → `CoreType` | `impl From<&LatticeType>` | `compile/lattice/types.rs:616` | L | `Const` value dropped; `Conditional` slot dropped (branches joined) |
| 22 | `ConcreteType` → `CoreType` | `impl From<&ConcreteType>` | `compile/lattice/types.rs:635` | L,S | struct `type_id` dropped, name **re-parsed** via `from_julia_name`; `Range{element}` → `Struct{"AbstractRange", [element]}` (element preserved since Issue #5916; `Range{Any}` keeps bare `Abstract(AbstractRange)`); `Function`/`Closure` names dropped |
| 23 | `ConstValue` → `ConcreteType` | `to_concrete_type` | `compile/lattice/types.rs:62` | L | the constant value itself |
| 24 | `ConcreteType` → `Option<String>` | `to_type_name` | `compile/lattice/types.rs:508` | S | many variants → `None` (`String`, `Char`, `Nothing`, `Tuple`, `Dict`, `Enum`, …) |
| 25 | `String` → `Option<ConcreteType>` | `from_type_name` | `compile/lattice/types.rs:566` | S | parametric names → `Struct { type_id: 0 }` (**type_id defaulted**); `"Int"`/`"UInt"` resolve per native word width |
| 26 | `JuliaType` → `ValueType` | `julia_type_to_value_type` | `compile/type_helpers.rs:228` | L,D | abstract → concrete representative or `Any` |
| 27 | `JuliaType` → `ValueType` (table) | `julia_type_to_value_type_with_table` | `compile/type_helpers.rs:437` | L,D | + struct table resolution |
| 28 | `JuliaType` → `ValueType` (ctx) | `julia_type_to_value_type_with_ctx` | `compile/core_compiler.rs:432` | L,D | + compiler context (Memory structs via `memory_struct_name_to_value_type`, `type_helpers.rs:396`) |
| 29 | `JuliaType` → `ValueType` (resolved) | `julia_type_to_value_type_resolved` | `compile/expr/infer/mod.rs:130` | L,D | fourth variant, inference-side |
| 30 | `ValueType` → `JuliaType` (×2) | `value_type_to_julia_type` (canonical) + dup | `vm/builtins_reflection/primitives.rs:7` (canonical), `compile/expr/infer/julia_type.rs:1273` (dup) | L,D | `vm/type_objects.rs` copy replaced by a thin delegating wrapper (Issue #5916; its `Union → Any` collapse was the disagreement — the canonical impl preserves unions, empty union → `JT::Bottom`); `ArrayOf(_)` element still **dropped** (→ bare `JT::Array`) |
| 31 | `ValueType` → `&'static str` | `value_type_to_type_name` | `compile/inference.rs:193` | S | feeds the string-keyed promotion tables |
| 32 | `String` → `Option<ValueType>` | `type_name_to_value_type` | `compile/inference.rs:215` | S | inverse of #31 for promotion results |
| 33 | `TypeExpr` → `String` | `Display` / `render_param_list` / `format_parameterized` / `as_simple_type_name` | `types/type_expr.rs` (Issue #5916) | S | compile-layer recursive renderers and collection simple-name helpers now delegate to `TypeExpr`; parameter-list joins are centralized |
| 34 | `TypeExpr` → `JuliaType` | `TypeExpr::{to_julia_type_lossy, substitute_to_julia_type_lossy}` | `types/type_expr.rs` (used by `compile/context.rs`, `compile/pipeline_ctx.rs`, `vm/type_objects.rs`) | S | **Issue #6720**: `to_julia_type_lossy` now routes through the structured `TypeExpr → CoreType → JuliaType` hub (`impl From<&TypeExpr> for CoreType`), so parametric params survive as `CoreType` structure instead of a string render+reparse; byte-identical projection for lowering shapes. `substitute_to_julia_type_lossy` now routes its `Parameterized` arm through the same hub (`substitute_to_core` + `core_type_to_julia_type`), so the substituted-param string round-trip is gone too. Leaves keep the single-name parse |
| 35 | `String` → `TypeExpr` | `TypeExpr::from_name` / `parse_type_expr_from_text` | `types/type_expr.rs:35`, `lowering/struct_.rs:1030` | S | unknown names become `TypeVar` |
| 36 | `TypeExpr` → `StaticType` | `StaticType::from_type_expr_lossy` | `aot/types.rs:504` (used by `aot/inference/engine/mod.rs`) | L | AoT struct field annotation projection now reuses the shared name parser before backend lowering |
| 37 | `JuliaType` ↔ `String` | `Display`/`name()` vs `from_name` / `from_name_or_struct` | `types/julia_type/display.rs:7`, `parsing.rs:447`, `parsing.rs:704` | S | `from_name_or_struct` **never fails** — unknown spellings silently become `Struct(name)` (~100 call sites) |
| 38 | `CoreType` ↔ `String` | `to_julia_name` / `from_julia_name` | `inference_core/type_core.rs:493` / `:624` | S | the *good* string bridge: parses `Union{}`, `Tuple{}`, `Type{}`, `Vararg{}`, `where` into structured forms |
| 39 | `ArrayElementType` → `ValueType` | `to_value_type` | `vm/value/array_element.rs:168` | — | |
| 40 | `ArrayElementType` ↔ `ConcreteType` | `convert_array_element_type` / `convert_concrete_to_array_element` | `compile/bridge.rs:403` / `:540` | L,S | ~~`UnionOf(String)` body re-parsed at `bridge.rs:475`~~ **resolved (Issue #6720)**: `UnionOf` now carries `Vec<JuliaType>`; `convert_union_array_element_members` canonicalizes structurally via `canonicalize_union`, no string re-parse |
| 41 | `ValueType` → `VarTypeTag` | `value_type_to_var_type_tag` | `vm/slot.rs:437` | L | slot-tag projection |
| 42 | `[JuliaType]` → `CoreType` (tuple sig) | `core_tuple_signature_from_julia_types` | `inference_core/dispatch_resolver.rs:61` | — | dispatch cache key (Issue #6336 canonical) |
| 43 | IR annotation → `JuliaType` | `as_julia_type` | `ir/core.rs:244` | L | `None` for typevars |
| 44 | `ConcreteType` lossy via `to_string` | `ConcreteType::from_type_name(&jt.to_string())` | `vm/builtins_reflection/mod.rs:2476` (+ `reflection_julia_type_to_concrete` `:2506`) | S,D | `JuliaType → Display string → ConcreteType` where structured #18/#19 exists |

---

## 3. Findings

### 3.1 (a) Conversion pairs that disagree (A→B→A ≠ identity)

1. **Lattice inversion through `ValueType`** — **half-resolved / half-documented**
   (Issue #5916 wave 2, see §3.5). The `ValueType`-side spelling of `Union{}`
   (`Union(vec![])`) now maps back to `Bottom` instead of `Top`;
   `LatticeType::Bottom → ValueType::Any → Top` remains as a *deliberate,
   pinned* codegen widening (the exact carrier was tried and reverted).
2. **`ConcreteType` string round-trip is not closed**:
   `from_type_name("String") = Some(String)` but `String.to_type_name() = None`
   (same for `Char`); and `Struct{name, type_id} → name → Struct{name,
   type_id: 0}` **zeroes the type_id** (`lattice/types.rs:537` vs `:602`).
3. **`JuliaType → CoreType` is non-injective**: `JT::Pairs` and
   `JT::Struct("Pairs")` map to the same `CoreType`, so the inverse (#2) must
   pick one spelling. Correctness currently rests on the Base-corpus pin test
   (`base_method_params_roundtrip_core_signature_issue_6336`), not on the
   types themselves.
4. ~~**`ConcreteType::Range{element} → CoreType::Abstract(AbstractRange)`**~~ —
   **RESOLVED** (Issue #5916 wave 2, see §3.5). The element is now preserved
   as `Struct{"AbstractRange", [element]}` (`lattice/types.rs:687`).
5. **`ValueType::Struct(type_id)` without a table** (#10) invents the name
   `"Struct#N"`; converting that onward (e.g. to a type name) leaks the
   synthetic spelling.

### 3.2 (b) Dead conversions

Exactly **one** found (everything else in the table has live call sites):

- `impl From<&crate::types::JuliaType> for aot::JuliaType`
  (`aot/types.rs:889`, 79 lines): zero call sites outside its own recursion.
  Verified by removal + `cargo check --all-targets --features "aot repl"` and
  `--features "cranelift repl"` (both clean). **Removed in this PR.**
  The live vm→AoT path is `JuliaType → StaticType` (#7/#8).

(Private dead conversions cannot survive here because the repo's
`-D warnings` clippy gate flags `dead_code`; only `pub` items and
feature-gated AoT code can hide. The remaining 4 pre-existing `dead_code`
warnings under `--features aot` are codegen/optimizer internals, not
conversions: `aot/codegen/aot_codegen/mod.rs:103`,
`aot/codegen/aot_codegen/control_flow.rs:230`, `aot/optimizer/cse.rs:29,66`.)

### 3.3 (c) String round-trips used where a structured path exists (bug-prone)

1. **`TypeExpr → JuliaType` (#34)** — formerly `TypeExpr::to_julia_type_lossy`
   rendered the structured `TypeExpr` to text and re-parsed it with
   `from_name_or_struct`, collapsing a parametric application's params into an
   opaque `JuliaType::Struct(String)` mid-flight. The genuinely structured target
   exists only in `CoreType`. **Resolved (Issue #6720):** added the direct
   `impl From<&TypeExpr> for CoreType` resolver (`inference_core/type_core/convert.rs`)
   and rerouted `to_julia_type_lossy` through the structured
   `TypeExpr → CoreType → JuliaType` hub (`core_type_to_julia_type(&CoreType::from(te))`),
   so parametric params now survive as real `CoreType` structure instead of a
   string round-trip. Behaviour is byte-identical to the old projection for
   lowering-produced shapes (pinned by
   `type_expr::tests::to_julia_type_lossy_matches_string_round_trip_issue_6720`).
   `substitute_to_julia_type_lossy` was rerouted through the same hub too
   (`substitute_to_core` + `core_type_to_julia_type`), so the substituted-param
   render+reparse is gone as well (Issue #6720, 3rd slice).
   Top-level bare type-var / runtime-expr **leaves** keep the single-name parse
   on purpose (an unresolved `T` must stay the `Struct("T")` placeholder, not be
   reinterpreted as a `TypeVar`).
2. **Promotion plumbing**: `ValueType → &str → promote → Option<ValueType>`
   (#31/#32) and the `ConcreteType` equivalent (#24/#25) used by
   `compile/tfuncs/arithmetic.rs:280,294`, `intrinsics.rs:59,273,581`,
   `abstract_interp/engine/mod.rs:4709–4733`. The promotion system is
   string-keyed *by design* (PROMOTION.md), but each round-trip inherits the
   §3.1.2 asymmetries. A `PrimitiveNumeric`-keyed fast path exists and should
   absorb these.
3. **Reflection**: `ConcreteType::from_type_name(&ty.to_string())`
   (`vm/builtins_reflection/mod.rs:2476`) — `JuliaType → Display → ConcreteType`
   right next to a structured fallback (`reflection_julia_type_to_concrete`).
4. **`from_name_or_struct` as a universal sink** (~100 call sites): it never
   fails, so typos / unstructurable spellings silently become user structs.
   `CoreType::from_julia_name` (#38) is strictly better (parses `Union`,
   `Tuple`, `Type{}`, `where`); new code should reach `JuliaType` only at the
   display boundary.
5. ~~**`ArrayElementType::UnionOf(String)`** (#40): a type embedded as source
   text inside the VM's element-type enum; re-parsed at `bridge.rs:475`.~~
   **Resolved (Issue #6720)**: `UnionOf` now carries structured
   `Vec<JuliaType>` members. The string body channel is gone; the bridge and
   `array_element_type_to_julia_type` canonicalize members structurally.

### 3.4 Duplicated parallel implementations (consolidation fodder)

| Conversion | Copies |
|---|---|
| `JuliaType → LatticeType` | **canonical established (Issue #5916 wave 3)**: `bridge::julia_type_to_lattice` / `julia_type_to_lattice_with_struct_table(ty, Option<&table>)` (`bridge.rs`). It resolves the three historical disagreements in favour of upstream-correct behaviour (empty `Union{}` → `Bottom`; union-with-`Any` → `Top`; abstract numerics preserved as their `ConcreteType` marker) and reuses `julia_type_to_concrete_type_lossy` for elements (pinned by `bridge::test_julia_type_to_lattice_*_issue_5916`). **4 sibling-owned copies remain to delegate** (none in this PR's scope): `analyzer.rs:696`, `engine/mod.rs:4354` (pass `Some(&self.struct_table)`), `expr_tfuncs.rs:695`, `builtins_reflection/mod.rs:2454`. |
| `JuliaType → ValueType` | **4** (`type_helpers.rs:228`, `:437`, `core_compiler.rs:432`, `expr/infer/mod.rs:130`) — already single-base: `:437`/`core_compiler.rs:432`(`_with_ctx`) delegate to `:228` for non-Struct/Union shapes; the ctx/inference variants add genuine capability (Memory structs, type-param resolution), not pure copies. The base `:228` is **not in this PR's scope** (`compile/type_helpers.rs`), so no further dedup here; the sibling `expr/infer/mod.rs:130` `_resolved` variant should delegate to it. |
| `ValueType → JuliaType` | **2** (canonical `builtins_reflection/primitives.rs:7`; dup `expr/infer/julia_type.rs:1273` in a #5922-owned file) — `type_objects.rs` copy now a thin delegating wrapper (Issue #5916) |
| `JuliaType → ConcreteType` | **1 structured canonical + 2 lattice-derived wrappers** (Issue #5916 wave 3 clarification): the structured impl is `bridge::julia_type_to_concrete_type_lossy` (now `pub(crate)` so siblings can reach it). The two "copies" `julia_type_to_concrete_or_any` (`analyzer.rs:807`, `engine/mod.rs:4446`) are **not** parallel structured impls — each routes through its host's `julia_type_to_lattice` and projects the result, so they will collapse automatically once those delegate to the canonical lattice bridge above. |
| `ConcreteType → JuliaType` | **1 canonical** `bridge::concrete_type_to_julia_type` — the engine's `concrete_type_to_julia` (`engine/mod.rs`) already delegates to `bridge::lattice_to_julia_type`, so the canonical is the single source of truth. Issue #6599 unified its braced-struct arm with the partial `lattice_to_parametric_julia_type` (both parse via `from_name_or_struct`). |
| `ValueType → LatticeType` | **2** families (`bridge.rs:20/196`, `struct_info.rs:141/153`) — `struct_info.rs` still maps empty `Union` to `Union(∅)`, not `Bottom` (#5922-owned file) |

### 3.5 Resolved round-trips (Issue #5916 wave 2, 2026-06-12)

1. **Lattice inversion through `ValueType` (was §3.1.1) — resolved for the
   `Union{}` carrier, documented for the rest.** `ValueType` has no dedicated
   Bottom variant, but the codebase has an established `Union{}` **carrier**:
   `julia_type_to_value_type` maps `JuliaType::Bottom →
   ValueType::Union(vec![])` (the empty union *is* `Union{}`).
   - **Fixed**: `ValueType::Union(vec![]) → LatticeType::Bottom`
     (`bridge.rs:155`, table-aware variant `:235`) — `Union{}` entering
     through `ValueType` no longer inverts to `Top`
     (`test_empty_union_value_type_is_bottom_issue_5916`), and the image of
     Bottom is still the identity of `join` / absorbing element of `meet`
     (`test_join_meet_laws_at_conversion_boundary_issue_5916`).
   - **Kept lossy, now pinned**: `LatticeType::Bottom → ValueType::Any`
     (`bridge.rs:281`). Mapping it to the exact carrier was implemented and
     **reverted**: in-progress recursive-call estimates surface `Bottom` at
     call sites (e.g. `Meta.unblock`'s call-site return type), and strict
     codegen consumers (`compile/expr/struct_.rs` field access, coercion)
     then reject inference-unreachable code that must still compile. The
     widening is a sound over-approximation; the inversion is pinned by test
     so any future change is deliberate.
   - The total `lattice_to_julia_type` now preserves `Bottom → JT::Bottom`
     (`JuliaType` *does* have a Bottom variant), so both JuliaType-direction
     conversions agree. The Issue #4679 arm in
     `lattice_to_parametric_julia_type` is **retained and stays necessary**:
     it is the only path that recovers `Union{}` for reflection callers,
     because the `ValueType` fallback deliberately widens.
   Non-empty unions whose arms fail to convert still widen to `Top` (up is
   the only sound direction for "unknown").
2. **`Range{element} → CoreType` element drop (was §3.1.4).**
   `ConcreteType::Range{element}` now converts to
   `CoreType::Struct { name: "AbstractRange", params: [element] }` — exactly
   the structured form `CoreType::from_julia_name("AbstractRange{T}")`
   produces, so the subtype engine's range-family rules apply unchanged
   (`Struct{"AbstractRange",[Int64]} <: Abstract(AbstractRange)` holds).
   `Range{Any}` keeps the bare `Abstract(AbstractRange)` because
   `AbstractRange{Any}` would be a *different* (invariant) claim. Caveat:
   the `ValueType::Range → Range{Int64}` element **default** in #10 remains —
   a float range that enters the lattice through the ValueType bridge still
   gets the Int64 guess (pre-existing; now visible to `CoreType` consumers,
   though no Base method dispatches on a parametric `AbstractRange{T}`
   annotation today).
3. **`ValueType → JuliaType` dedupe (partial, §3.4).** `vm/type_objects.rs`'s
   copy is a thin wrapper over the canonical
   `builtins_reflection/primitives.rs` impl. Disagreement resolved in favor
   of the canonical/upstream behavior: unions are preserved structurally
   (`Union{...}`; empty → `Union{}`), instead of collapsing to `Any`.

### 3.6 Canonical conversions established (Issue #5916 wave 3, 2026-06-12)

This wave establishes the **single in-scope source of truth** for two
`JuliaType`-source directions in `compile/bridge.rs` and resolves the
disagreements among the historical parallel copies in favour of upstream
Julia. **Wave 4 (2026-06-12) collapsed the sibling-owned copies onto the
canonical converters**: `compile/type_stability/analyzer.rs`,
`compile/abstract_interp/engine/mod.rs`, and `vm/builtins_reflection/mod.rs`
now delegate (their local conversion bodies and `…_to_concrete` projections
are deleted — net −5 hand-rolled conversion impls). **Wave 5 (Issue #5922,
2026-06-12) delegated the last copy**, `compile/expr/infer/expr_tfuncs.rs`:
its tfunc-argument `julia_type_to_lattice` now routes the shared concrete
mapping through the canonical converter and keeps only explicitly documented
adapter pinnings (see below).

1. **`JuliaType → LatticeType` — canonical `bridge::julia_type_to_lattice` /
   `julia_type_to_lattice_with_struct_table(ty, Option<&table>)`.** The four
   historical copies disagreed on three points; the canonical impl adopts the
   upstream-correct resolution of each, **verified against `julia` 1.12**:
   - **Empty `Union{}` → `Bottom`.** Julia:
     `typeof(Union{}) == Core.TypeofBottom`, so `Union{}` *is* `Bottom`. (The
     reflection copy produced `LatticeType::Union(∅)`; the `expr_tfuncs` copy
     collapsed the whole union to `Top` — both wrong.) A union containing `Any`
     widens to `Top`; a non-empty concrete union is kept as `LatticeType::Union`.
   - **Abstract numeric supertypes preserved** (`Number`/`Real → Number`,
     `Integer`/`Signed`/`Unsigned → Integer`, `AbstractFloat`) instead of
     widening to `Top`, so callers can still specialize.
   - **Struct resolution parameterized**: with a `struct_table`, a
     `JuliaType::Struct(name)` resolves to its registered `type_id`; without
     one, it keeps the structured `ConcreteType::Struct { type_id: 0 }`
     placeholder (better than the engine copy's bare `Top`).
   - Element/parameter conversion reuses the structured
     `julia_type_to_concrete_type_lossy` so the lattice and ConcreteType
     directions can never drift (pinned by
     `bridge::test_julia_type_to_lattice_agrees_with_concrete_lossy_issue_5916`
     and five sibling `test_julia_type_to_lattice_*_issue_5916` tests).
   - **Sibling-owned call sites — resolved in wave 4** (sibling delegation PR,
     2026-06-12). The canonical entry points were generalized to
     `julia_type_to_lattice_with_struct_resolver(ty, Option<&dyn Fn(&str) ->
     Option<usize>>)` so hosts with a *different* struct-table type (the
     engine's `StructTypeInfo` vs the compiler's `context::StructInfo`) can
     still supply struct-id resolution; element/parameter positions (array
     element, tuple element, union member) now recurse through the
     resolver-aware projection `julia_type_to_concrete_or_any_with_struct_resolver`,
     so a `Vector{MyStruct}` annotation keeps its registered `type_id` and a
     `Vector{Union{...}}` keeps the element union. Two latent canonical bugs
     were fixed while delegating: bare `JuliaType::Array` now lowers to
     `Concrete(Array{Any})` (the lossy `_` fallback had collapsed it to
     `Concrete(Any)`, drifting from every sibling copy), and `lossy` gained the
     matching bare-`Array` arm.
     - **DONE** `compile/type_stability/analyzer.rs` → thin wrapper over
       `julia_type_to_lattice(ty)` (gains abstract-numeric preservation,
       correct tuple arity for non-concrete elements, union-element
       preservation; its `julia_type_to_concrete_or_any` deleted).
     - **DONE** `compile/abstract_interp/engine/mod.rs` → thin wrapper over
       `julia_type_to_lattice_with_struct_resolver(ty, Some(&|name|
       self.struct_table.get(name).map(|i| i.type_id)))`. The engine's
       struct-not-in-table → `Top` fallback turned out to be **load-bearing**
       (abstract families like `AbstractDict` are spelled `Struct(name)`;
       treating them as concrete structs broke the `dict_mergewith` fixture),
       so the canonical adopts it whenever a resolver is supplied: resolver
       present + name unresolved → `Top`; no resolver → `Struct{type_id: 0}`
       placeholder (wave-3 table-free behavior, unchanged). Its
       `julia_type_to_concrete_or_any` was deleted.
     - **DONE** `vm/builtins_reflection/mod.rs`
       (`reflection_julia_type_to_lattice`) → thin wrapper over
       `julia_type_to_lattice(ty)` (fixes its empty-`Union(∅)` divergence;
       union-containing-`Any` → `Top`; `Real`/`Signed`/`Unsigned` keep abstract
       markers; plain struct names keep `Struct{type_id: 0}`;
       `reflection_julia_type_to_concrete` deleted).
     - **DONE (wave 5, Issue #5922)** `compile/expr/infer/expr_tfuncs.rs` →
       delegates the shared concrete mapping to `julia_type_to_lattice(ty)`
       (its local `concrete_type_from_julia_type` deleted; the union-loss bug
       is fixed by adopting the canonical union handling). The tfunc-argument
       edge keeps **explicit pinned divergences**, each unit-tested:
       dispatch-deferral edges (`Struct(name)`/`Signed`/`Unsigned`/`Bottom` →
       `Top`, so struct/abstract args keep deferring to method dispatch; the
       `Bottom` pin is independent of the canonical Bottom edge, Issue #6523),
       type objects (`TypeOf(T) → DataType{name}` — load-bearing for
       typemin/typemax/zeros/ones), and legacy result pinnings
       (`AbstractString`/`AbstractChar`/`AbstractArray`, `NamedTuple{}`,
       ranges → `Range{Any}`, `Module`/`Function`/`IO`/metaprogramming nodes/
       `Generator{Any}`/`Enum{name}`; element positions recurse through the
       wrapper).
     - **DONE (Issue #6600)**: shrank the pinned divergence surface. Added an
       **adapter-level pin audit** (`pin_audit_load_bearing_arms_diverge_dead_arms_match`)
       that, per pinned arm, drives every julia-path adapter entry point under
       the local pin vs. canonical delegation (via a `#[cfg(test)]`-only
       delegation hook) and asserts the dead-vs-load-bearing verdict. The audit
       proved every remaining pin is **load-bearing at the adapter level** (the
       deferral, type-object, range, abstract-string/char/array, and the
       `Module`/`Function`/`IO`/`IOBuffer`/`NamedTuple`/metaprogramming/`Pairs`/
       `Generator`/`Enum` "legacy result" pins all change at least one routed
       adapter output — chiefly the `min`/`max`/`reverse` identity tfuncs, which
       return the concrete pin instead of canonical `Top → Float64`/original
       JuliaType). The **only dead pin was `TupleOf(_) → Tuple{}`**: it was
       **removed** (now delegated to canonical, keeping structured
       `Tuple{…}` elements). This is adapter-neutral because every julia-path
       entry point projects any tuple back through `julia_type_from_concrete_type`
       (collapsing `Tuple{…}` → bare `JuliaType::Tuple`) and the lone
       element-sensitive rule (`length → Const(Int64(n))`) is widened to
       `JuliaType::Int64` regardless of `n`. (`Vector`/`Matrix` keep their arm:
       it is load-bearing whenever the element is a deferral type, e.g.
       `Vector{MyStruct}` stays `Array{Any}` not `Array{Struct{0}}`.)
     - **RESOLVED (Issue #6523, wave 5)**: `JuliaType::Bottom` (the canonical
       spelling of `Union{}`) now lowers to `LatticeType::Bottom` in the
       canonical converter, agreeing with the non-canonical `Union(vec![])`
       spelling (it previously fell through `_ => Top` — the widest type
       instead of the narrowest; pre-existing since wave 3 and shared by the
       historical engine/analyzer copies, while the reflection copy produced
       a bogus `Struct{"Union{}"}`). The `LatticeType::Bottom → ValueType::
       Any` carrier widening (§3.5) is unchanged, and the `expr_tfuncs`
       tfunc-argument `Bottom → Top` dispatch-deferral pin above is an
       independent, deliberate divergence. Observable effect is
       engine/analyzer-side: a multi-method callee's recorded `Union{}`
       return snapshot (consulted via `MethodSig.return_julia_type` —
       single-method calls are inferred directly in lattice space) re-enters
       inference as the identity of `join`, so a caller branching between
       such a call and a `Float64` infers `Float64` instead of `Any`
       (upstream-verified; fixture
       `type_inference/bottom_return_snapshot_join_6523.jl`, unit
       `test_julia_type_to_lattice_bottom_variant_is_bottom_issue_6523`).
2. **`JuliaType → ConcreteType` — collapsed to 1 structured canonical + 1
   canonical lattice projection.** `bridge::julia_type_to_concrete_type_lossy`
   is the structured canonical (`pub(crate)`). The two sibling
   `julia_type_to_concrete_or_any` projections (`analyzer.rs`,
   `engine/mod.rs`) were **deleted** in wave 4; their role is filled by the
   canonical `bridge::julia_type_to_concrete_or_any_with_struct_resolver`.
3. **`JuliaType → ValueType` — confirmed already single-base, no in-scope
   dedup.** The base impl is `compile/type_helpers.rs:228`
   (`julia_type_to_value_type`); `:437` (`_with_table`) and
   `core_compiler.rs:432` (`_with_ctx`) already delegate to it for non-Struct/
   Union shapes and only add capability (Memory structs, type-param
   resolution). **Still deferred** (owned by the parallel infer refactor):
   `compile/expr/infer/mod.rs:130` (`_resolved`) delegating to it as well.

---

## 4. Recommended target & phased migration

**Canonical representation: `CoreType`.** It is the only one that is
structurally complete (params, both typevar bounds, UnionAll, Vararg, value
params), already owns subtype/intersect/join/specificity, and is already the
serialization source of truth for method signatures (Issue #6336). The others
become **views**:

| Representation | Future role |
|---|---|
| `CoreType` | canonical semantic type (hub of all conversions) |
| `JuliaType` | display / user-facing spelling + legacy API view |
| `ValueType` (+`ArrayElementType`) | codegen/VM carrier projection (intentionally lossy, fine) |
| `LatticeType` | inference lattice *wrapper* (`Const`/`Conditional`/`Top`/`Bottom`) whose payload becomes `CoreType` instead of `ConcreteType` |
| `TypeExpr` | lowering AST (kept; gains a direct `→ CoreType` resolver) |
| `aot::StaticType` | AoT backend projection (kept; routes via `CoreType` — the `Array`/`Matrix`/`Vector` family now projects *only* through `CoreType`, #6598) |
| `aot::JuliaType` | shrink toward deletion (entry `From` impl already dead/removed). **#6598 residual**: the enum stays as the AoT **IR type carrier** — `IrFunction` return/param types, `VarRef`, `ConstValue::get_type`, cranelift codegen (`julia_type_to_cranelift`), and rooting (`julia_type_requires_rooting_model`) all key off it. Full deletion needs the IR to carry `CoreType`/`StaticType` instead, which is the larger #6599 structural change; for now its only *conversions* (`primitive_numeric`/`is_numeric`/… and `From<&aot::JuliaType> for CoreType`) route through the shared `CoreType` taxonomy. |

### Phases

1. **Inventory + dead-code removal** (this PR).
2. **De-duplicate** the §3.4 parallel impls into one module each
   (`compile/bridge.rs` for lattice-side, `vm/` for reflection-side), keeping
   behavior pinned by the existing tests; fix the `ConcreteType` string
   asymmetries (§3.1.2) while doing so.
3. **Hub through `CoreType`**: replace direct `JuliaType ↔ ConcreteType`
   edges with `A → CoreType → B` (both `From` impls already exist toward
   `CoreType`; the missing piece is `CoreType → ConcreteType`). Add round-trip
   property tests per edge.
   **Issue #6599 (landed, 2026-06-15):** the missing `impl From<&CoreType> for
   ConcreteType` is added (`compile/lattice/types.rs`, round-trip-pinned with a
   documented lossy-arm contract — Bottom/typevar/UnionAll/Vararg/value-param/
   abstract-family widening, struct params re-embedded in the name string), and
   the `JuliaType → ConcreteType` direction (`julia_type_to_concrete_type_lossy`)
   now routes through it (`ConcreteType::from(&CoreType::from(ty))`), recovering
   container structure the old `_ => Any` fallthrough dropped. The reverse
   `ConcreteType → JuliaType` (`concrete_type_to_julia_type`) is **kept direct
   and deferred to Phase 4**: rerouting it through `CoreType` would lose the
   load-bearing reflection special-cases (`DataType` #4843, `Enum` #2863, struct
   `type_id`, bare-name dispatch deferral), since `core_type_to_julia_type` lacks
   `TypeOf`/`Enum` arms and `CoreType` drops `type_id`.
4. **Structured parametrics for the views**: give `LatticeType` a
   `CoreType`-payload variant and route `TypeExpr → CoreType` directly,
   eliminating the §3.3.1 forced string round-trip; replace
   `ArrayElementType::UnionOf(String)` with a structured form.
   **Issue #6720 (landed, 2026-06-16):** the `ArrayElementType::UnionOf(String)`
   sub-item is done — the variant now carries `Vec<JuliaType>` members.
   Construction sites store members directly (or lift a body string via
   `union_from_body` at string-source boundaries); display preserves member
   order; and the bridge / `array_element_type_to_julia_type` canonicalize the
   members structurally through `canonicalize_union`, removing the
   `bridge.rs` `Union{...}` re-parse (#40).
   **Issue #6720 (landed, 2026-06-16, 2nd slice):** the `TypeExpr → CoreType`
   resolver sub-item is done — `impl From<&TypeExpr> for CoreType` added and
   `to_julia_type_lossy` rerouted through `TypeExpr → CoreType → JuliaType`,
   eliminating the §3.3.1 forced string round-trip (#34) for parametric
   applications (params now land as `CoreType::Struct{name, params}` /
   `Tuple` / canonical `Union`). The remaining Phase 4 item — giving
   `LatticeType` a `CoreType` payload (the full `Concrete(ConcreteType) →
   Concrete(CoreType)` swap) — is **deferred to a multi-PR effort**: it is
   semantically lossy (drops `type_id` / `Closure` captures / `Enum`, breaking
   #5085 / closures / #2863) and the design end-state (Phase 6) is
   `ConcreteType` = `CoreType` + lattice-only variants, not a raw `CoreType`
   payload. See the analysis in the Issue #6720 thread.
   **Issue #6720 (landed, 2026-06-16, 3rd slice):** `substitute_to_julia_type_lossy`
   was rerouted through the same hub (`substitute_to_core` +
   `core_type_to_julia_type`), removing its substituted-param render+reparse;
   `#34` now has no remaining `TypeExpr` string round-trip in either projection
   method (only the on-purpose single-name leaf parse remains).
5. **Promotion keys**: migrate `to_type_name`/`from_type_name` promotion
   round-trips onto `PrimitiveNumeric`/`CoreType` keys; keep the string tables
   only as the user-extensible `promote_rule` surface.
6. **Retire `ConcreteType`** as a separate enum (becomes a thin newtype or
   alias over `CoreType` + the few lattice-only variants such as `Closure`).
   **Design landed (Issue #6720, 2026-06-17):** the wrapper approach
   (`ConcreteType = Core(CoreType)` + lattice-only carriers
   `Function`/`Closure`/`ComposedFunction`/`Enum`, with struct `type_id`
   resolved from the name) is specified in
   [CONCRETETYPE_RETIREMENT.md](./CONCRETETYPE_RETIREMENT.md), including the
   variant inventory, `type_id` resolution strategy, hazards, and the multi-PR
   migration slices. "Enrich `CoreType` directly" was rejected (it would pollute
   the 1706-use serialized semantic core).

Related issues: #5915 (vm/mod.rs split), #5917 (type_core split, done),
#5921 (comparison unification), #5922 (tfuncs), #6336 (MethodSig canonical
core_signature), #6495/#6496 (CoreType migration follow-ups).
