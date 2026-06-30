# Design — #6599 Phase 3: make `CoreType` the `JuliaType ↔ ConcreteType` conversion hub

- **Issue**: #6599 ("[#5916] ValueType↔JuliaType 重複ペアの乖離統合と ValueType のビュー化(表現削減本体)")
- **Parent epic**: #5916 (型表現の統合・変換面削減)
- **Date**: 2026-06-15
- **Scope chosen**: roadmap **Phase 3** ("hub through `CoreType`") — the realistic, sliceable
  representation-reduction body. NOT the Phase-4 `ValueType`/`LatticeType` demotion
  (5267 `ValueType` uses), which is explicitly out of scope.

## Background

The codebase has six type representations (`docs/vm/TYPE_REPRESENTATIONS.md` §1). The
roadmap (§4) names **`CoreType`** (`inference_core/type_core/repr.rs:182`) the canonical,
structurally-complete type and makes every other representation a *view* of it. Phase 3 of
that roadmap is: **replace the direct `JuliaType ↔ ConcreteType` conversion edges with
`A → CoreType → B`**, so there is a single structural source of truth and the two
representations cannot drift apart.

`#6599`'s sub-parts 1 and 2 already landed (`ValueType↔JuliaType` unification — #5916 wave 2;
`LatticeType→JuliaType` pair unification — PR `40e10ba84`, PR #6646). What remains for this
effort is Phase 3's hub work.

The roadmap notes the only missing piece is the `CoreType → ConcreteType` edge (the two
`From`-impls *toward* `CoreType` already exist: `JuliaType → CoreType` at
`inference_core/type_core/convert.rs:3`, `ConcreteType → CoreType` at
`compile/lattice/types.rs:635`).

## Key finding: the work is asymmetric

The two direct edges differ sharply in reroute safety:

| Direct edge | Location | Reroute safety |
|---|---|---|
| `JuliaType → ConcreteType` (`julia_type_to_concrete_type_lossy`) | `compile/bridge.rs:540`, 8 internal callers | **Safe.** Lossy arms already pinned by `test_julia_type_to_lattice_agrees_with_concrete_lossy_issue_5916`. |
| `ConcreteType → JuliaType` (`concrete_type_to_julia_type`) | `compile/bridge.rs:1945`, 10 internal callers | **Hazardous.** Carries load-bearing reflection special-cases — `DataType` (#4843), `Enum` (#2863), struct `type_id`, bare-name dispatch deferral — that a `ConcreteType → CoreType → JuliaType` round-trip would lose (`CoreType` drops `type_id`; `core_type_to_julia_type` has no `TypeOf`/`Enum` arm). |

Therefore Phase 3 reroutes only the safe `JuliaType → ConcreteType` direction. The reverse
direction's full reroute is **deferred to Phase 4** (documented in-place with the reason); we
only tighten its parametric precision here, without rerouting it.

## Architecture

Add `impl From<&CoreType> for ConcreteType` as the new canonical downward edge. Reroute
`julia_type_to_concrete_type_lossy` to `ConcreteType::from(&CoreType::from(julia_type))`.
Keep `concrete_type_to_julia_type` as a direct edge (a user-facing projection, not a hub
candidate yet), optionally improving its braced-parametric precision.

### `CoreType → ConcreteType` lossy-arm contract (Slice A)

`ConcreteType` is strictly less expressive than `CoreType` (no `Bottom`, no typevars/
`UnionAll`/`Vararg`/value-params, limited abstract families, params embedded in name
strings, `type_id` link). The new impl is **intentionally lossy** at these arms, each
documented and round-trip-tested:

| `CoreType` variant | `ConcreteType` image | Lossy? |
|---|---|---|
| `Bottom` | `Any` | yes — over-approximation (`ConcreteType` has no Bottom) |
| `Any` | `Any` | no |
| `Primitive(p)` | matching primitive `ConcreteType` | no |
| `Abstract(Number/Integer/AbstractFloat)` | matching `ConcreteType` abstract | no |
| `Abstract(other families)` | `Any` | yes — `ConcreteType` lacks Real/Signed/… |
| `AbstractUser { name, .. }` | `Any` | yes — no concrete image |
| `Struct { name, params }` | `Struct { name: <rendered braced name>, type_id: 0 }` | yes — `type_id` synthesized as 0 (resolved later by the lattice/struct table); params re-embedded in the name string |
| `Tuple(elems)` | `Tuple { elements }` (recursive) | no |
| `NamedTuple(fields)` | `NamedTuple { fields }` (recursive) | no |
| `Union(members)` | `UnionOf(members)` (recursive) | no |
| `Vararg`/`VarargLen` | element (or `Any`) | yes — `ConcreteType` has no Vararg |
| `TypeVar` | upper bound (or `Any`) | yes — typevars are compile-time |
| `Value(_)` | `Any` | yes — value params have no concrete home |
| `UnionAll { body, .. }` | image of `body` (quantifier dropped) | yes |
| `TypeOf(inner)` | `DataType { name: <rendered inner name> }` | yes — string round-trip |
| `Module(name)` | `Module { name }` | no |
| `Named(name)` | `Struct { name, type_id: 0 }` | yes — fallback |

The bare-`Array` special case (`Array` with no element → `Array{Any}`, #5916) must be
preserved: `CoreType::Struct { name: "Array", params: [] }` → `ConcreteType::Array { element:
Box::new(Any) }`.

## Migration slices (one PR each; per-slice tests + full suite)

- **Slice A — add `impl From<&CoreType> for ConcreteType`.** New code, **zero callers** →
  safest. Implements the lossy-arm contract above. Tests: a `CoreType → ConcreteType →
  CoreType` round-trip property test over `ConcreteType`-projectable values (identity on the
  non-lossy arms), plus explicit unit assertions for each lossy arm (documenting the
  widening). No behavior change anywhere (nothing calls it yet).
- **Slice B — reroute `julia_type_to_concrete_type_lossy` through `CoreType`.** Change its
  body to `ConcreteType::from(&CoreType::from(ty))` (preserving the bare-`Array` special
  case). The 8 internal call sites are unchanged. **Behavior must stay identical**, pinned by
  the existing `test_julia_type_to_lattice_agrees_with_concrete_lossy_issue_5916` (must stay
  green) plus a new direct round-trip test for the rerouted edge. Eliminates one divergent
  direct edge.
- **Slice D — tighten `concrete_type_to_julia_type` parametric handling (optional, no
  reroute).** Improve precision of braced parametric struct spellings on the direct
  `ConcreteType → JuliaType` edge (extend the Issue #6599 `from_name_or_struct` handling), so
  it agrees with the lattice→julia pair on more shapes. Does **not** reroute through CoreType.
  Tests: extend `test_lattice_to_julia_type_pair_agrees_on_braced_struct_issue_6599`.

Slice C (rerouting `julia_type_to_concrete_or_any_with_struct_resolver`) is intentionally
skipped — that function already routes through the lattice layer, not a direct edge.

## Load-bearing behavior to preserve (must stay green every slice)

- `LatticeType::Bottom → ValueType::Any` deliberate widening (`bridge.rs:281`, #5916 §3.5) —
  lives on the `LatticeType → ValueType` edge, **outside this scope**; must not be perturbed.
- The #4679 reflection `Bottom` preservation (`lattice_to_parametric_julia_type` /
  `lattice_to_julia_type` keep `Bottom → JuliaType::Bottom`).
- The dispatch-deferral pins `julia_type_to_lattice_pins_dispatch_deferral_edges_to_top`
  (`Struct(name)`/`Signed`/`Unsigned`/`Bottom` → `Top` at the tfunc-argument adapter).
- `DataType` (#4843) and `Enum` (#2863) precision on the `ConcreteType → JuliaType` direct
  edge — preserved by NOT rerouting that direction.
- All existing `bridge` round-trip and `test_julia_type_to_lattice_*_issue_5916` tests.

## Testing strategy

- Each slice runs the **full** workspace suite from the root:
  `timeout 1800 cargo nextest run --release` (never `| tail`; the #5966-class hazards and
  cross-binary state mean targeted runs are insufficient before merge — controller gate).
- Slice A adds round-trip property tests for `CoreType ↔ ConcreteType` (identity on non-lossy
  arms; explicit assertions documenting each lossy arm).
- Slice B keeps `test_julia_type_to_lattice_agrees_with_concrete_lossy_issue_5916` green and
  adds a direct round-trip test for the rerouted edge.
- Slice D extends `test_lattice_to_julia_type_pair_agrees_on_braced_struct_issue_6599`.
- `cargo clippy --all-targets -- -D warnings` clean each slice.
- `scripts/test_aot.sh` (AoT gate) on any slice that could touch the AoT projection path
  (Slice A's new edge is reachable from `aot::StaticType → CoreType` consumers).

## Done criteria

- `impl From<&CoreType> for ConcreteType` exists with a documented, round-trip-tested
  lossy-arm contract.
- `julia_type_to_concrete_type_lossy` routes through `CoreType`; the `#5916` agreement test
  stays green; the direct `JuliaType → ConcreteType` divergent edge is gone.
- (Slice D) `concrete_type_to_julia_type` braced-parametric precision improved.
- Full suite + clippy + AoT gate green.
- `docs/vm/TYPE_REPRESENTATIONS.md` §3.4/§4 updated: the `JuliaType → ConcreteType` edge now
  hubs through `CoreType`; the `ConcreteType → JuliaType` reroute is recorded as Phase-4
  deferred with the load-bearing reasons.

## Out of scope (future phases)

- Rerouting `concrete_type_to_julia_type` through `CoreType` (Phase 4 — needs
  `core_type_to_julia_type` to gain `TypeOf`/`Enum`/struct-param arms and to preserve
  `type_id`).
- Giving `LatticeType` a `CoreType`-payload variant; demoting `ValueType` toward a thin view
  (Phase 4; 5267 `ValueType` uses).
- Replacing `ArrayElementType::UnionOf(String)` with a structured form (Phase 4).
- Re-evaluating the `Bottom → ValueType::Any` widening (separate task; #6532 has landed so the
  premise is in place, but it is not part of this Phase-3 effort).
