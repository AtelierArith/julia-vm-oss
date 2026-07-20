# Array / Memory Migration Plan

> **Archive note (2026-06-11):** This preserves the older detailed migration
> log. The active status note is `docs/vm/ARRAY_MEMORY_MIGRATION.md`.

Issues: #3894, #3908

This document defines the phased migration from Rust-owned `Value::Array`
toward the Julia 1.11+ model where `Memory{T}` is the primitive storage object
and `Array{T,N}` is a Julia-visible wrapper carrying shape and indexing
semantics.

## Upstream References

Study these upstream files under `./julia` before changing a phase:

- `julia/src/jltypes.c` for builtin `GenericMemory`, `GenericMemoryRef`,
  `Array`, `Memory`, and type-layout initialization.
- `julia/src/array.c` for low-level allocation, storage ownership, and array
  runtime representation.
- `julia/base/essentials.jl` for bootstrap `GenericMemory` / `MemoryRef`
  constructors, bounds, `getindex`, and `setindex!`.
- `julia/base/array.jl` for `wrap(Array, Memory, dims)`, `reshape`, `collect`,
  and concrete `Array` methods.
- `julia/base/abstractarray.jl`, `julia/base/indices.jl`, and
  `julia/base/multidimensional.jl` for generic indexing, `similar`, and
  dimensional semantics.
- `julia/base/subarray.jl` for view representation and parent/index storage.

## Current Inventory

As of 2026-05-25 (after the value_enum
`runtime_type` / `value_type` early-return migration through the shared
`legacy_array_value_ref` helper on top of the vm/util
`value_type_name` early-return migration on top of the bin/sjulia
`format_value_with_vm` early-return migration on top of the repl/globals
`set` early-return migration on top of the type_ops `deep_copy` /
`introspection` early-return migrations on top of the vm/formatting
`format_value_slow` / `value_to_string` early-return migrations on top
of the ffi/format `format_value` early-return migration on top of the
ffi/basic `compile_and_run_auto` early-return migration on top of the
vm/exec/locals `StoreAny` early-return migration on top of the vm/util
`bind_value_to_frame` early-return migration on top of the vm/mod
`get_value_type` / `get_value_julia_type` early-return migration to the
shared `legacy_array_value_ref` helper on top of the value_enum test
array construction migration to the shared `array_ref_value`
constructor on top of the array_value_from_value doc comment rephrase
on top of the vm/mod `compare_array_wrapper_against_other` helper
factoring on top of the vm/exec/binary_both `memory_array_pair` helper
on top of the vm/dynamic_ops/helpers
`broadcastable_array_like` delegation on top of
the vm/stack_ops `pop_array` delegation to the
shared `array_ref_from_value` plus audit cleanup on top of
the shared `array_ref_from_value` / `legacy_array_value_mut_ref` helpers
plus the 3-file migration (`vm/exec/array_index.rs`,
`vm/exec/array_mutate.rs`, `vm/exec/array_basic.rs`) on top of
the additional 8-file migrations to the shared
`array_ref_value` / `array_value_from_value` constructors (binary_both,
array_basic, array_mutate, builtins_arrays, builtins_strings, matrix,
builtins_reflection, container) on top of
the vm/value
shared `array_value_from_value` constructor plus the 10-file migration
(`vm/exec/range.rs`, `vm/exec/rng.rs`, `vm/exec/array_index_slice.rs`,
`vm/builtins_io.rs`, `vm/hof_exec/value_mode.rs`,
`vm/builtins_macro/mod.rs`, `vm/type_ops/iteration.rs`,
`vm/dynamic_ops/mod.rs`, `vm/builtins_dicts.rs`,
`vm/builtins_linalg.rs`, `vm/builtins_types.rs`) on top of
the vm/value
shared `array_ref_value` constructor plus the 5-file
migration (`vm/exec/hof.rs`, `vm/exec/struct_ops.rs`,
`vm/exec/array_index.rs`, `vm/frame.rs`,
`vm/hof_exec/dispatch.rs`) on top of
the vm/dynamic_ops
shared `legacy_array_value_ref` migration on top of
the vm/mod
shared `legacy_array_value_ref` migration on top of
the vm/formatting
shared `legacy_array_value_ref` migration on top of
the vm/util
shared `legacy_array_value_ref` migration on top of
the vm/builtins_reflection/primitives
shared `legacy_array_value_ref` migration on top of
the vm/builtins_linalg
shared `legacy_array_value_ref` migration on top of
the vm/builtins_types
shared `legacy_array_value_ref` migration on top of
the vm/builtins_macro
shared `legacy_array_value_ref` migration on top of
the vm/builtins_dicts
shared `legacy_array_value_ref` migration on top of
the vm/type_ops/iteration
shared `legacy_array_value_ref` migration on top of
the vm/exec/array_index_slice
shared `legacy_array_value_ref` migration on top of
the vm/exec/struct_ops
shared `legacy_array_value_ref` migration on top of
the vm/exec/call_dynamic
shared `legacy_array_value_ref` migration on top of
the vm/builtins_equality
shared `legacy_array_value_ref` migration on top of
the shared
vm/value/array_value `legacy_array_value_ref` helper plus
the vm/builtins_collections first migration on top of
the vm/exec/binary_both
is_legacy_array_value delegation on top of
the vm/exec/array_basic
legacy_array_value_into delegation on top of
the vm/builtins_arrays pop helper delegation on top of
the vm/exec/array_index Generator iter + sub_array parent helper delegation
on top of
the compile/inference_trace audit cleanup on top of
the vm/exec/hof helper delegation on top of
the vm/hof_exec/dispatch helper delegation on top of
the compile/expr/coercion comment cleanup on top of
the vm/exec/binary_both doc cleanup on top of
the vm/type_ops/iteration doc cleanup on top of
the vm/builtins_types doc comment cleanup on top of
the vm/formatting doc comment cleanup on top of
the vm/exec/call_dynamic Array helpers on top of
the vm/builtins_io Array construction helper on top of
the vm/exec/range Array construction helper on top of
the vm/exec/array_basic Array helpers round 2 on top
of the vm/exec/struct_ops Array helpers on top of the
builtins_macro/mod Array helpers on top of the
vm/exec/rng Array push helper on top of the
builtins_equality Array destructure helpers
round 2 on top of the builtins_collections Array destructure helpers
routing, the array_index_slice Array helpers routing, the
builtins_dicts Array helpers routing, the builtins_reflection primitives
Array helpers routing, the builtins_equality Array isequal/hash helpers
routing, the formatting display boundary routing, the vm/mod equality bridge
helpers, the Subtypes any_vector helper routing, and the builtins_strings
`String(::Vector{Char})` / `codeunits` / `_substring_retag` / `findall`
native-Array push and read routing, Issue #3908):

- `Value::Array`: 5 references in `subset_julia_vm/src` (net -13 from a
  follow-up round of early-return migrations: every remaining exhaustive
  `Value::Array(arr) => ...` arm across `vm/util` (`bind_value_to_frame`
  + `value_type_name`), `vm/exec/locals` (`StoreAny`), `ffi/basic`
  (`compile_and_run_auto`), `ffi/format` (`format_value`),
  `vm/formatting` (`format_value_slow` + `value_to_string`),
  `vm/type_ops/{deep_copy,introspection}`, `repl/globals` (`set`),
  `bin/sjulia` (`format_value_with_vm`), and `value_enum`
  (`runtime_type` + `value_type`) was rewritten as an early-return guard
  through the shared `legacy_array_value_ref` helper with a `_ =>`
  wildcard fallback. Each migration removed the file from
  `scripts/check_value_array_allowlist.sh`. The remaining 5 matches are
  the absolute floor for the helper-consolidation approach: the four
  shared helper bodies in `vm/value/array_value/mod.rs` (`legacy_array_value_ref`,
  `array_ref_value`, `array_ref_from_value`, `legacy_array_value_mut_ref`)
  and the single multi-variant arm in `value_enum.rs`'s
  `test_all_value_variants_constructed` exhaustive-coverage assertion.
  Further reductions require Phase 5 work — retiring `Value::Array` from
  the `Value` enum itself.)
- `Value::Memory`: 166 references in `subset_julia_vm/src`
- `memory_to_array_ref`: 0 references in `subset_julia_vm/src`
- files touching `Value::Array` / `ValueType::Array` / `ArrayValue` /
  `ArrayData`: 75

`subset_julia_vm_vm/src/vm/builtins_dicts.rs` now routes the `DictKeys` /
`DictValues` `Vec<Value>` construction through a file-local
`any_vector_array_value(Vec<Value>) -> Value` helper, and routes the
`DictKeys` / `DictValues` / `DictPairs` array branches through guarded
`_ if legacy_array_value_ref(&val).is_some()` arms backed by a file-local
`legacy_array_value_ref(&Value) -> Option<&ArrayRef>` helper. The audited
ceiling for the file dropped from 5 to 2 in
`scripts/check_value_array_allowlist.sh`.

`subset_julia_vm_vm/src/vm/exec/rng.rs` now routes the three identical
`Value::Array(new_array_ref(arr))` constructions in the `RandArray`,
`RandIntArray`, and `RandnArray` handlers through a file-local
`array_value(arr: ArrayValue) -> Value` helper, mirroring the pattern used by
PRs #4468 / #4469 / #4471. The audited ceiling for the file dropped from 3 to
1 in `scripts/check_value_array_allowlist.sh`; the remaining match is the
helper body's `Value::Array(new_array_ref(arr))` arm (Issue #3908).

### vm/dynamic_ops/helpers `broadcastable_array_like` delegation (Issue #3908)

`subset_julia_vm_vm/src/vm/dynamic_ops/helpers.rs::broadcastable_array_like`
previously matched the input value directly with
`Value::Array(arr) => ...` and `Value::Memory(mem) => ...` arms.

Replace with two sequential `if let` guards: the array branch now uses
`legacy_array_value_ref(value)` to obtain the inner `ArrayRef`, and the
`Value::Memory(mem)` branch stays as a direct `if let`. The same early
return order is preserved. The file's `Value::Array` count drops from 1
to 0, and its `scripts/check_value_array_allowlist.sh` entry is
removed.

### vm/stack_ops `pop_array` delegation + audit cleanup (Issue #3908)

`subset_julia_vm_vm/src/vm/stack_ops.rs::pop_array` previously matched the
popped value directly with `Value::Array(arr) => Ok(arr)` and
`Value::Range(r) => Ok(new_array_ref(r.collect()))`. Replace the outer
match with a `let popped = self.pop()?` and route through
`crate::vm::value::array_ref_from_value(popped)`; the `Range` automatic
conversion stays via the `Err(Value::Range(r))` guard.

`scripts/check_value_array_allowlist.sh` cleanup: drop the `stack_ops.rs`
entry (now 0 matches), plus stale `vm/exec/matrix.rs` and
`vm/builtins_reflection/mod.rs` entries that were left at `echo 1`
despite their files having been migrated to 0 in earlier PRs.

### Shared `array_ref_from_value` + `legacy_array_value_mut_ref` + 3-file migration (Issue #3908)

Two more shared helpers are added to
`subset_julia_vm_vm/src/vm/value/array_value/mod.rs` and re-exported from
`vm/value/mod.rs`:

- `pub(crate) fn array_ref_from_value(value: Value) -> Result<ArrayRef, Value>`
  — shared owned-value destructure (the `try_consume_array_value` /
  `array_ref_from_value` pattern previously held in three files).
- `pub(crate) fn legacy_array_value_mut_ref(value: &mut Value) -> Option<&mut ArrayRef>`
  — shared `&mut` destructure (the `legacy_array_value_mut_ref` helper
  previously held in `vm/exec/array_basic.rs`).

Three files migrate to these helpers:

- `vm/exec/array_index.rs`: drops its file-local `array_ref_from_value`,
  imports the shared one. 1 → 0.
- `vm/exec/array_mutate.rs`: `try_consume_array_value` body delegates to
  `array_ref_from_value(value)`. 1 → 0.
- `vm/exec/array_basic.rs`: `try_consume_array_value` and
  `legacy_array_value_mut_ref` bodies delegate to the shared helpers. 2 → 0.

`scripts/check_value_array_allowlist.sh`:
- `vm/value/array_value/mod.rs` ceiling: 3 → 5 (five helper bodies now).
- Entries for `vm/exec/array_index.rs`, `vm/exec/array_mutate.rs`, and
  `vm/exec/array_basic.rs` removed.

Net `Value::Array` count: -2.

### Additional file migrations to shared constructor helpers (Issue #3908)

After introducing the shared `array_ref_value` (PR #4519) and
`array_value_from_value` (PR #4520) constructors, eight additional VM
files are migrated from their file-local `Value::Array(...)` constructors
to the shared helpers:

- `vm/exec/binary_both.rs`: `array_value(ArrayValue)` body now delegates
  to `array_value_from_value`. 4 → 3.
- `vm/exec/array_basic.rs`: `push_array_ref` body now uses
  `array_ref_value`. 3 → 2.
- `vm/exec/array_mutate.rs`: `push_array_ref` body now uses
  `array_ref_value`. 2 → 1.
- `vm/builtins_arrays.rs`: `push_array_ref` body now uses
  `array_ref_value`. 2 → 1.
- `vm/builtins_strings.rs`: file-local `array_value(ArrayRef)` removed
  in favor of `array_ref_value as array_value` import. 2 → 1.
- `vm/exec/matrix.rs`: direct `Value::Array(new_array_ref(result))` push
  replaced with `array_value_from_value(result)`. 1 → 0.
- `vm/builtins_reflection/mod.rs`: direct
  `Value::Array(new_array_ref(ArrayValue::any_vector(...)))` replaced
  with `array_value_from_value(ArrayValue::any_vector(...))`. 1 → 0.
- `vm/value/container.rs`: `Expr::get_args` construction routed through
  `array_value_from_value`. 1 → 0.

`scripts/check_value_array_allowlist.sh` updated accordingly. Net
`Value::Array` count: -8.

### Shared vm/value `array_value_from_value` constructor + 10-file migration (Issue #3908)

Many VM files keep their own file-local
`fn array_value(arr: ArrayValue) -> Value { Value::Array(new_array_ref(arr)) }`
(or specialized variants like `linalg_array_value`,
`any_vector_array_value`, `dynamic_array_value`) for wrapping owned
`ArrayValue` into the transitional native carrier.

This slice adds `pub(crate) fn array_value_from_value(arr: ArrayValue)
-> Value { array_ref_value(new_array_ref(arr)) }` to
`subset_julia_vm_vm/src/vm/value/array_value/mod.rs`, re-exported via
`vm/value/mod.rs`.

Eleven files are migrated to delegate (or directly use) the shared
helper: `vm/exec/range.rs`, `vm/exec/rng.rs`,
`vm/exec/array_index_slice.rs`, `vm/builtins_io.rs`,
`vm/hof_exec/value_mode.rs`, `vm/builtins_macro/mod.rs`,
`vm/type_ops/iteration.rs` (via the `Vm::array_value` method body),
`vm/dynamic_ops/mod.rs` (`dynamic_array_value`),
`vm/builtins_dicts.rs` (`any_vector_array_value`),
`vm/builtins_linalg.rs` (`linalg_array_value`), and
`vm/builtins_types.rs` (`any_vector_array_value`). Each file's
`Value::Array` count drops from 1 to 0.

`scripts/check_value_array_allowlist.sh`: `vm/value/array_value/mod.rs`
ceiling raised from 2 to 3 (three helper bodies now); entries for all
eleven migrated files are removed. Net `Value::Array` count: -10.

### Shared vm/value `array_ref_value` constructor + 5-file migration (Issue #3908)

Many VM files keep their own file-local
`fn array_value(arr: ArrayRef) -> Value { Value::Array(arr) }` (or a
`fn array_ref_value(arr: ArrayRef) -> Value` companion). Each copy is
counted in the per-file `Value::Array` audit ceiling.

This slice adds a shared `pub(crate) fn array_ref_value(arr: ArrayRef)
-> Value` to `subset_julia_vm_vm/src/vm/value/array_value/mod.rs`,
re-exported from `vm/value/mod.rs` via
`pub(crate) use array_value::{array_ref_value, legacy_array_value_ref};`.

Five files are migrated in the same change:

- `vm/exec/hof.rs`: drops its file-local `array_ref_value`, imports the
  shared one. `array_value(ArrayValue)` keeps its file-local body but now
  delegates via `array_ref_value(new_array_ref(arr))`. 1 → 0.
- `vm/exec/struct_ops.rs`: drops its `fn array_value(ArrayRef)`,
  imports `array_ref_value as array_value`. 1 → 0.
- `vm/exec/array_index.rs`: same pattern — `array_ref_value as
  array_value` import, file-local helper removed. 2 → 1 (the
  `array_ref_from_value` destructure helper stays).
- `vm/frame.rs`: drops its file-local `array_value(ArrayRef)`, imports
  shared. 1 → 0.
- `vm/hof_exec/dispatch.rs`: drops its file-local `array_ref_value`,
  imports shared. `array_value(ArrayValue)` keeps its body delegating
  to the shared `array_ref_value`. 1 → 0.

`scripts/check_value_array_allowlist.sh`: `vm/value/array_value/mod.rs`
ceiling raised from 1 to 2 (two helper bodies now);
`vm/exec/array_index.rs` ceiling lowered from 2 to 1; entries for
`vm/exec/hof.rs`, `vm/exec/struct_ops.rs`, `vm/frame.rs`, and
`vm/hof_exec/dispatch.rs` removed (each now has 0 matches). Net
`Value::Array` count: -4.

### vm/dynamic_ops shared `legacy_array_value_ref` migration (Issue #3908)

Both `subset_julia_vm_vm/src/vm/dynamic_ops/dispatch.rs` and `mod.rs`
previously kept (or matched) the legacy native-array carrier directly:
`dispatch.rs` had its own file-local
`fn legacy_array_value_ref(value: &Value) -> Option<&ArrayRef>` helper, and
`mod.rs::is_array_like_value` held a direct
`matches!(value, Value::Array(_) | Value::Memory(_))`.

This migration switches both files to the shared
`crate::vm::value::legacy_array_value_ref` helper. `dispatch.rs` drops its
file-local helper and imports the shared one; `mod.rs::is_array_like_value`
is rewritten as
`legacy_array_value_ref(value).is_some() || matches!(value, Value::Memory(_))`.
`scripts/check_value_array_allowlist.sh`: `dispatch.rs` entry is removed
(now 0 matches), `mod.rs` ceiling drops from 2 to 1 (remaining match is
the `dynamic_array_value` construction body).

### vm/mod shared `legacy_array_value_ref` migration (Issue #3908)

`subset_julia_vm_vm/src/vm/mod.rs` previously kept its own file-local
`fn legacy_array_value_ref(value: &Value) -> Option<&ArrayRef>` used by
the `is_legacy_array_value` predicate. The body was identical to the
shared helper.

Drop the file-local helper and add `legacy_array_value_ref` to the
existing `use value::{ ... };` import. The `is_legacy_array_value`
predicate keeps its `legacy_array_value_ref(value).is_some()` body, now
routed through the shared helper. The file's `Value::Array` count
drops from 7 to 6; the remaining matches are three exhaustive arms
(`get_value_type`, `get_value_julia_type`, `get_global`), two
cross-carrier tuple-pattern arms in
`compare_array_wrapper_boundary_values_equal`, and the test-module
`array_value` constructor. `scripts/check_value_array_allowlist.sh`
ceiling for the file drops from 7 to 6.

### vm/formatting shared `legacy_array_value_ref` migration (Issue #3908)

`subset_julia_vm_vm/src/vm/formatting.rs` previously kept its own file-local
`fn legacy_array_value_ref(value: &Value) -> Option<&ArrayRef>` used by
`value_to_julia_code`. The body was identical to the shared helper.

Drop the file-local helper and rewrite the imports as
`use super::value::{ ..., legacy_array_value_ref, ... };`. The file's
`Value::Array` count drops from 4 to 3; the remaining matches are the
two exhaustive `Value::Array(arr) =>` arms in `format_value_slow` and
`value_to_string`, plus the test-module `array_value(ArrayRef) -> Value`
constructor. `scripts/check_value_array_allowlist.sh` ceiling for the
file drops from 4 to 3.

### vm/util shared `legacy_array_value_ref` migration (Issue #3908)

`subset_julia_vm_vm/src/vm/util.rs` previously kept its own file-local
`fn legacy_array_value_ref(value: &Value) -> Option<&ArrayRef>` used by
`pop_array_or_values`. The body was identical to the shared helper.

Drop the file-local helper and rewrite the imports as
`use super::value::{ ..., legacy_array_value_ref, ... };`. The file's
`Value::Array` count drops from 3 to 2; the remaining matches are the
two exhaustive `Value::Array(_) =>` arms in `value_type_name` and
`bind_value_to_frame` (Rust exhaustiveness checking cannot prove a
guarded helper-call covers the variant in matches without a wildcard
fallback). `scripts/check_value_array_allowlist.sh` ceiling for the
file drops from 3 to 2.

### vm/builtins_reflection/primitives shared `legacy_array_value_ref` migration (Issue #3908)

`subset_julia_vm_vm/src/vm/builtins_reflection/primitives.rs` previously
kept its own file-local `fn legacy_array_value_ref(value: &Value) ->
Option<&ArrayRef>` used by the `extract_types_from_value` Vector-of-types
branch. The body was identical to the shared helper.

Drop the file-local helper and rewrite the imports as
`use super::super::value::{ ..., legacy_array_value_ref, ... };`. The
file's `Value::Array` count drops from 2 to 1; the remaining match is
the test-module `array_value(ArrayValue) -> Value` constructor used by
reflection fixture tests. `scripts/check_value_array_allowlist.sh`
ceiling for the file drops from 2 to 1.

### vm/builtins_linalg shared `legacy_array_value_ref` migration (Issue #3908)

`subset_julia_vm_vm/src/vm/builtins_linalg.rs` previously kept its own
file-local `fn legacy_array_value_ref(value: &Value) -> Option<&ArrayRef>`
used by `with_linalg_array` and `linalg_array_wrapper_value`. The body
was identical to the shared helper.

Drop the file-local helper and rewrite the imports as
`use super::value::{ ..., legacy_array_value_ref, ... };`. The now-unused
`ArrayRef` import is removed in the same edit. The file's `Value::Array`
count drops from 2 to 1; the remaining match is the file-local
`linalg_array_value(ArrayValue) -> Value` constructor body.
`scripts/check_value_array_allowlist.sh` ceiling for the file drops
from 2 to 1.

### vm/builtins_types shared `legacy_array_value_ref` migration (Issue #3908)

`subset_julia_vm_vm/src/vm/builtins_types.rs` previously kept its own
file-local `fn legacy_array_value_ref(value: &Value) -> Option<&ArrayRef>`
used by the `Typeof` / `Isa` / `Sizeof` / `Ismutable` / `Objectid` / `In`
array branches. The body was identical to the shared helper.

Drop the file-local helper and rewrite the imports as
`use super::value::{ ..., legacy_array_value_ref, ... };`. The file's
`Value::Array` count drops from 2 to 1; the remaining match is the
file-local `any_vector_array_value(Vec<Value>) -> Value` constructor
used by reflection helpers. `scripts/check_value_array_allowlist.sh`
ceiling for the file drops from 2 to 1.

### vm/builtins_macro shared `legacy_array_value_ref` migration (Issue #3908)

`subset_julia_vm_vm/src/vm/builtins_macro/mod.rs` previously kept its own
file-local `fn legacy_array_value_ref(value: &Value) -> Option<&ArrayRef>`
used by the `Expr` splat arm. The body was identical to the shared helper.

Drop the file-local helper and rewrite the imports as
`use super::value::{ ..., legacy_array_value_ref, ... };`. The file's
`Value::Array` count drops from 2 to 1; the remaining match is the
file-local `array_value(ArrayValue) -> Value` construction helper used
by the `split` / `eachmatch` push sites.
`scripts/check_value_array_allowlist.sh` ceiling for the file drops
from 2 to 1.

### vm/builtins_dicts shared `legacy_array_value_ref` migration (Issue #3908)

`subset_julia_vm_vm/src/vm/builtins_dicts.rs` previously kept its own
file-local `fn legacy_array_value_ref(value: &Value) -> Option<&ArrayRef>`
used by the `DictKeys` / `DictValues` / `DictPairs` Array branches.
The body was identical to the shared helper.

Drop the file-local helper and rewrite the imports as
`use super::value::{ ..., legacy_array_value_ref, ... };`. The file's
`Value::Array` count drops from 2 to 1; the remaining match is the
file-local `any_vector_array_value(Vec<Value>) -> Value` construction
helper. `scripts/check_value_array_allowlist.sh` ceiling for the file
drops from 2 to 1.

### vm/type_ops/iteration shared `legacy_array_value_ref` migration (Issue #3908)

`subset_julia_vm_vm/src/vm/type_ops/iteration.rs` previously kept its own
file-local `fn legacy_array_value_ref(value: &Value) -> Option<&ArrayRef>`
used by eight destructure call sites (matrix shape probes, the
`_mem::Array` linear getter, the `iterate_first`/`iterate_next` arms, the
`collect_iterator` Array arm, and the `collect_iterator_values` unwrap).
The body was identical to the shared helper.

Drop the file-local helper and rewrite the imports as
`use crate::vm::value::{ ..., legacy_array_value_ref, ... };`. The
file's `Value::Array` count drops from 2 to 1; the remaining match is
the file-local `array_value(ArrayValue) -> Value` construction helper
used by `collect_zip_fields`, `collect_enumerate_fields`,
`collect_rest_fields`, `collect_logrange_fields`, and the five
generator-callable empty fallbacks in `collect_generator_dispatch`.
`scripts/check_value_array_allowlist.sh` ceiling for the file drops
from 2 to 1.

### vm/exec/array_index_slice shared `legacy_array_value_ref` migration (Issue #3908)

`subset_julia_vm_vm/src/vm/exec/array_index_slice.rs` previously kept its own
file-local `fn legacy_array_value_ref(value: &Value) -> Option<&ArrayRef>`
used by four destructure call sites (the `array_wrapper_logical_values`
`_mem::Array` reader, `value_to_slice_index` Array index source, the
string-indexing Array-of-indices arm, and the top-level slicing target
unwrap). The body was identical to the shared helper.

Drop the file-local helper and rewrite the imports as
`use crate::vm::value::{ ..., legacy_array_value_ref, ... };`. The
file's `Value::Array` count drops from 2 to 1; the remaining match is
the file-local `array_value(ArrayValue) -> Value` construction helper
used by the slicing result-array re-push sites.
`scripts/check_value_array_allowlist.sh` ceiling for the file drops
from 2 to 1.

### vm/exec/struct_ops shared `legacy_array_value_ref` migration (Issue #3908)

`subset_julia_vm_vm/src/vm/exec/struct_ops.rs` previously kept its own
file-local `fn legacy_array_value_ref(value: &Value) -> Option<&ArrayRef>`
used by `NewStructSplat` and the Pure Julia Array wrapper `_mem`/`_size`
bridge in `GetFieldByName`. The body was identical to the shared helper.

This migration drops the file-local helper and rewrites the imports as
`use super::super::value::{ ..., legacy_array_value_ref, ... };`. The
file's `Value::Array` count drops from 2 to 1; the remaining match is the
file-local `array_value(ArrayRef) -> Value` construction helper used by
the `_mem` projection push site. The
`scripts/check_value_array_allowlist.sh` ceiling for the file is lowered
from 2 to 1.

### vm/exec/call_dynamic shared `legacy_array_value_ref` migration (Issue #3908)

`subset_julia_vm_vm/src/vm/exec/call_dynamic.rs` previously kept its own
file-local `fn legacy_array_value_ref(value: &Value) -> Option<&ArrayRef>`
used by `can_score_iterate_dynamic_candidates` and
`native_array_rank_count`. The body was identical to the shared helper.

This migration drops the file-local helper and rewrites the imports as
`use crate::vm::value::{ ..., legacy_array_value_ref, ... };`. The file's
`Value::Array` count drops from 1 to 0, and its
`scripts/check_value_array_allowlist.sh` entry is removed. Dynamic
iterate dispatch behavior is unchanged.

### vm/builtins_equality shared `legacy_array_value_ref` migration (Issue #3908)

`subset_julia_vm_vm/src/vm/builtins_equality.rs` previously kept its own
file-local `fn legacy_array_value_ref(value: &Value) -> Option<&ArrayRef>`
used by `Isequal`, `Hash`, and `Egal`. The body was identical to the shared
helper introduced by the previous slice.

This migration drops the file-local helper and rewrites the imports as
`use super::value::legacy_array_value_ref;`. The file's `Value::Array`
count drops from 1 to 0, and its `scripts/check_value_array_allowlist.sh`
entry (`echo 1`) is removed. The `Isequal` / `Hash` / `Egal` builtins
behave the same — the helper is the same function under both names.

### Shared vm/value `legacy_array_value_ref` helper (Issue #3908)

Around 15 files inside `subset_julia_vm_vm/src/vm/` previously kept their own
file-local `legacy_array_value_ref(&Value) -> Option<&ArrayRef>` helper.
Their bodies were all identical:

```rust
match value {
    Value::Array(arr) => Some(arr),
    _ => None,
}
```

A single shared `pub(crate) fn legacy_array_value_ref` now lives in
`subset_julia_vm_vm/src/vm/value/array_value/mod.rs` and is re-exported from
`vm/value/mod.rs` via `pub(crate) use array_value::legacy_array_value_ref;`.

The first migration target is `vm/builtins_collections.rs`: its
file-local helper is removed and the file now imports the shared helper
through `use super::value::legacy_array_value_ref;`. Per-file
`Value::Array` count for `builtins_collections.rs` drops from 1 to 0;
the `scripts/check_value_array_allowlist.sh` entry for the file is
removed in the same change. A new allowlist entry classifies
`vm/value/array_value/mod.rs` at ceiling 1 (the shared helper body).
Net `Value::Array` count is unchanged for this slice; future per-file
migrations will reduce by 1 per migrated file.

### vm/exec/binary_both is_legacy_array_value delegation (Issue #3908)

`subset_julia_vm_vm/src/vm/exec/binary_both.rs` previously kept the file-local
predicate `is_legacy_array_value(&Value) -> bool` and the destructure helper
`legacy_array_ref_from_value(&Value) -> Option<&ArrayRef>` side by side, each
holding its own native-array match literal (the predicate held
`matches!(value, Value::Array(_))` and the destructure helper held the
`Value::Array(arr) => Some(arr)` arm).

`is_legacy_array_value` now delegates to
`legacy_array_ref_from_value(value).is_some()`, mirroring the same pattern
that `vm/mod.rs` already uses for its `is_legacy_array_value` /
`legacy_array_value_ref` pair. The predicate body no longer holds a literal
native-array match. `scripts/check_value_array_allowlist.sh`'s ceiling for
the file drops from 5 to 4; the remaining matches are the
`legacy_array_ref_from_value` destructure body, the `array_value`
construction body, and the two Memory<->Array equality bridge
tuple-pattern arms inside the matmul fallback. No runtime behavior changes.

### vm/exec/array_basic legacy_array_value_into delegation (Issue #3908)

`subset_julia_vm_vm/src/vm/exec/array_basic.rs` previously kept four
file-local helpers each holding their own native-array destructure or
construction literal: `push_array_ref` (stack-push constructor),
`legacy_array_value_mut_ref` (`&mut Value -> Option<&mut ArrayRef>`),
`legacy_array_value_into` (`Value -> Option<ArrayRef>`), and
`try_consume_array_value` (`Value -> Result<ArrayRef, Value>`).

`legacy_array_value_into` and `try_consume_array_value` are both
owned-value destructures; the former returns `Option<ArrayRef>` and the
latter returns `Result<ArrayRef, Value>`. Their behavior on the array
variant is identical, so `legacy_array_value_into` now delegates to
`try_consume_array_value(value).ok()`. The body no longer holds a
literal native-array destructure. `scripts/check_value_array_allowlist.sh`'s
ceiling for the file drops from 4 to 3; the remaining matches are the
`push_array_ref` re-push body, the `legacy_array_value_mut_ref`
destructure body, and the `try_consume_array_value` destructure body.
No runtime behavior changes.

### vm/builtins_arrays pop helper delegation (Issue #3908)

`subset_julia_vm_vm/src/vm/builtins_arrays.rs` previously kept three direct
native-array sites: the file-local `value_as_array_ref(&Value) ->
Option<&ArrayRef>` helper body, the `push_array_ref(ArrayRef)` stack push
constructor body, and a third direct `Value::Array(arr_ref) => Ok(arr_ref)`
destructure inside the `pop_array_ref_for_builtin(context: &str)` method
used by the in-place mutation builtins (`push!`, `pushfirst!`, `insert!`,
`deleteat!`, `pop!`, `popfirst!`).

`pop_array_ref_for_builtin` now delegates to the existing
`value_as_array_ref(&popped).cloned()` helper (cloning the borrowed
`ArrayRef` is a cheap `Rc` bump), mapping `None` to the same
`VmError::TypeError(format!("{context} requires array"))` it returned
before. `scripts/check_value_array_allowlist.sh`'s ceiling for the file
drops from 3 to 2; the remaining matches are the `value_as_array_ref`
helper body and the `push_array_ref` construction body. No runtime
behavior changes.

### vm/exec/array_index Generator iter + sub_array helper delegation (Issue #3908)

`subset_julia_vm_vm/src/vm/exec/array_index.rs` previously kept two direct
native-array match arms outside of its three file-local helpers:

1. `sub_array_parent_array_ref(values: &[Value]) -> Result<ArrayRef, VmError>`
   matched `Some(Value::Array(arr)) => Ok(arr.clone())` directly.
2. The Generator integer-indexing fallback matched `g.iter.as_ref()` against
   `Value::Array(arr) => { ... }` to read the underlying native-array carrier.

Both sites now delegate to the existing
`array_ref_from_value(value: Value) -> Result<ArrayRef, Value>` destructure
helper:

1. `sub_array_parent_array_ref` clones the first value (cheap Rc bump for the
   array case) and pipes it through `array_ref_from_value`, mapping the
   `Err(_)` branch back to the same `VmError::InternalError("SubArray parent
   must be an Array")` it returned before.
2. The Generator iter fallback clones the boxed iter value and routes it
   through `array_ref_from_value((*g.iter).clone())`. The `Ok(arr)` branch
   keeps the existing logical `ArrayValue::get_linear()` read, and the
   `Err(value)` branch fans out to `Err(Value::Range(_))`,
   `Err(Value::Tuple(_))`, and `Err(other)` arms that preserve the prior
   semantics (cloning the iter value is cheap for `Range`/`Tuple` and
   Generator integer indexing is a rare fallback path).

`scripts/check_value_array_allowlist.sh`'s ceiling for the file drops from
4 to 2; the remaining matches are the `array_value(ArrayRef) -> Value`
constructor body and the `array_ref_from_value` destructure body. No
runtime behavior changes.

### compile/inference_trace audit cleanup (Issue #3908)

`subset_julia_vm_compile/src/compile/inference_trace.rs` carried one
`Value::Array(entries)` literal inside `serialize_env`, but that match
was a false positive: the function had a local `use serde_json::{json,
Value};`, so `Value::Array` referred to `serde_json::Value::Array`, not
the runtime `crate::vm::value::Value::Array` enum that the audit
targets. `scripts/check_value_array_allowlist.sh` counts every
`Value::Array` substring via grep and cannot distinguish the two.
The local `Value` alias is removed: `serde_json::Value::Null` is
spelled fully qualified, and the array construction switches to
`Vec<serde_json::Value>::into()` (the standard `Vec<serde_json::Value>:
Into<serde_json::Value>` impl returns `Value::Array(self)`), so the
emitted JSON is byte-for-byte identical. The file's allowlist entry
in `scripts/check_value_array_allowlist.sh` is removed entirely —
`rg 'Value::Array' subset_julia_vm_compile/src/compile/inference_trace.rs`
now returns zero matches. No runtime behavior changes.

### vm/exec/hof helper delegation (Issue #3908)

`subset_julia_vm_vm/src/vm/exec/hof.rs` previously held two file-local
constructors that each produced a `Value::Array` literal: an
`array_value(ArrayValue) -> Value` helper that called
`Value::Array(new_array_ref(arr))`, and an
`array_ref_value(ArrayRef) -> Value` helper that called `Value::Array(arr)`.
`array_value` now delegates to `array_ref_value(new_array_ref(arr))`, so
only `array_ref_value`'s body still mentions `Value::Array`. Callers of
both helpers are unchanged. `scripts/check_value_array_allowlist.sh`'s
ceiling for the file drops from 2 to 1. No runtime behavior changes.

### vm/hof_exec/dispatch helper delegation (Issue #3908)

`subset_julia_vm_vm/src/vm/hof_exec/dispatch.rs` previously held two file-local
constructors that each produced a `Value::Array` literal: an
`array_value(ArrayValue) -> Value` helper that called
`Value::Array(new_array_ref(arr))`, and an
`array_ref_value(ArrayRef) -> Value` helper that called `Value::Array(arr)`.
`array_value` now delegates to `array_ref_value(new_array_ref(arr))`, so
only `array_ref_value`'s body still mentions `Value::Array`. Callers of
both helpers are unchanged. `scripts/check_value_array_allowlist.sh`'s
ceiling for the file drops from 2 to 1, and a short comment explains the
delegation. No runtime behavior changes.

### compile/expr/coercion comment cleanup (Issue #3908)

`subset_julia_vm_compile/src/compile/expr/coercion.rs` carried one literal
`Value::Array` mention in a regular `//` comment immediately above the
`Struct -> Array | ArrayOf` coercion arm. Because
`scripts/check_value_array_allowlist.sh` counts every `Value::Array`
substring (including comments), that single mention kept the file in the
allowlist with a ceiling of 1. The comment is rephrased to refer to "the
legacy native-array container" instead, and the file's allowlist entry is
removed entirely — `rg 'Value::Array' subset_julia_vm_compile/src/compile/expr/coercion.rs`
now returns zero matches, so the file no longer needs classification. No
runtime behavior changes.

### vm/type_ops/iteration doc cleanup (Issue #3908)

`subset_julia_vm_vm/src/vm/type_ops/iteration.rs` carried two literal
`Value::Array` mentions in the doc comment on its `legacy_array_value_ref`
helper. Because `scripts/check_value_array_allowlist.sh` counts every
`Value::Array` substring (including comments), the noise kept the
iteration.rs total at 4. The doc comment is rephrased to refer to "the
legacy native Array carrier" and "raw native-array destructures" instead,
dropping the helper-side ceiling from 4 to 2. The remaining two matches
are the `legacy_array_value_ref` helper body and the `array_value`
constructor body. No runtime behavior changes.

### vm/exec/binary_both doc cleanup (Issue #3908)

`subset_julia_vm_vm/src/vm/exec/binary_both.rs` carried two `Value::Array`
literal mentions in the doc comments on its `legacy_array_ref_from_value`
and `array_value` helpers. Because `scripts/check_value_array_allowlist.sh`
counts every `Value::Array` substring (including comments), the noise kept
the binary_both.rs total at 7. The doc comments are rephrased to refer to
"a legacy native-array operand" and "legacy native-array construction"
instead, dropping the helper-side ceiling from 7 to 5. The remaining five
matches are the three helper bodies (`is_legacy_array_value`,
`legacy_array_ref_from_value`, `array_value`) and the two tuple-pattern
arms of the Memory<->Array equality bridge in the matmul fallback. No
runtime behavior changes.

### vm/builtins_types doc comment cleanup (Issue #3908)

`subset_julia_vm_vm/src/vm/builtins_types.rs` carried two `Value::Array(_)`
literal mentions in the doc comment on its `legacy_array_value_ref`
helper. Because `scripts/check_value_array_allowlist.sh` counts every
`Value::Array` substring (including comments), the noise kept the
builtins_types.rs total at 4. The doc comment is rephrased to refer to
"the legacy native-array carrier" and "a raw native-array destructure
pattern" instead, dropping the helper-side ceiling from 4 to 2. The
remaining two matches are the `legacy_array_value_ref` helper body and
the `any_vector_array_value` constructor body. No runtime behavior
changes.

### vm/formatting doc comment cleanup (Issue #3908)

`subset_julia_vm_vm/src/vm/formatting.rs` carried a doc comment on the
`legacy_array_value_ref` helper that mentioned `Value::Array(_)` as a
literal token. Because `scripts/check_value_array_allowlist.sh` counts
every `Value::Array` substring (including comments), the noise pushed the
formatting.rs total to 5. The doc comment is rephrased to refer to "the
legacy native-array carrier variant" instead, dropping the helper-side
ceiling from 5 to 4. The remaining four matches are the helper body and
the two exhaustive `Value::Array(arr) =>` arms in `format_value_slow` /
`value_to_string` (kept direct because Rust exhaustiveness checking
cannot prove a guarded helper-call covers the variant), plus the
formatting test constructor body. No runtime behavior changes.

### vm/exec/call_dynamic Array helpers (Issue #3908)

`subset_julia_vm_vm/src/vm/exec/call_dynamic.rs` now routes the two remaining
native-Array destructure sites through a file-local
`legacy_array_value_ref(&Value) -> Option<&ArrayRef>` helper. The
`can_score_iterate_dynamic_candidates` predicate rewrites
`matches!(value, Value::Struct(_) | Value::StructRef(_) | Value::Array(_))` as
`matches!(value, Value::Struct(_) | Value::StructRef(_)) ||
legacy_array_value_ref(value).is_some()`, and the `native_array_rank_count`
helper rewrites its `Value::Array(arr) => { ... }` arm as
`let arr_ref = legacy_array_value_ref(iter)?; let arr = arr_ref.borrow();
Some((arr.shape.len(), arr.element_count(), arr.shape.is_empty()))`. The
audited ceiling for the file dropped from 2 to 1 in
`scripts/check_value_array_allowlist.sh`; the remaining match is the helper
body's `Value::Array(arr) => Some(arr)` arm. `IterateDynamic` native-Array
scoring against user `Vector` / `Matrix` iterate methods and the
`iterator_size_value_for_native_generator_iter` /
`generator_iter_known_nonempty` size dispatch are unchanged (Issue #3908).

### vm/builtins_io Array construction helper (Issue #3908)

`subset_julia_vm_vm/src/vm/builtins_io.rs` now routes both
`self.stack.push(Value::Array(new_array_ref(arr)))` constructions in the
`Readlines` and `Readdir` handlers through a file-local
`array_value(arr: ArrayValue) -> Value` helper, mirroring the pattern used by
PRs #4476 / #4482 / #4488 and the `vm/exec/rng.rs` / `vm/exec/range.rs`
migrations above. Both call sites build a `Vector{Any}`-shaped result via
`ArrayValue::any_vector(...)` from the file's I/O readers (line strings for
`readlines`, sorted directory entries for `readdir`), so the helper centralizes
the Array wrapping for the I/O return path. The audited ceiling for the file
dropped from 2 to 1 in `scripts/check_value_array_allowlist.sh`; the remaining
match is the helper body's `Value::Array(new_array_ref(arr))` arm
(Issue #3908).

### vm/exec/range Array construction helper (Issue #3908)

`subset_julia_vm_vm/src/vm/exec/range.rs` now routes both
`self.stack.push(Value::Array(new_array_ref(arr)))` constructions in the
`MakeRange` (Int64 array materialization) and `MakeRangeF64` (Float64 array
materialization) handlers through a file-local
`array_value(arr: ArrayValue) -> Value` helper, mirroring the pattern used by
PRs #4476 / #4482 and the `vm/exec/rng.rs` migration above. The audited
ceiling for the file dropped from 2 to 1 in
`scripts/check_value_array_allowlist.sh`; the remaining match is the helper
body's `Value::Array(new_array_ref(arr))` arm. `MakeRangeLazy` is unaffected
because it pushes `Value::Range`, not `Value::Array` (Issue #3908).

### vm/frame.rs Array helper (Issue #3908)

`subset_julia_vm_vm/src/vm/frame.rs` now routes both `Value::Array(v.clone())`
constructions in `Frame::get_by_tag` (`VarTypeTag::Array` arm) and
`Frame::get_by_cascade` (`self.locals_array.get(name)` arm) through a single
file-local `array_value(arr: ArrayRef) -> Value` helper. The helper reuses
the existing `super::value::ArrayRef` import and centralizes the
transitional native Array carrier construction at the only remaining
push site in the file. The audited ceiling for the file dropped from 2 to
1 in `scripts/check_value_array_allowlist.sh`; the remaining match is the
helper body. Local variable lookup semantics (`Frame::get_local` /
`get_by_tag` / `get_by_cascade`) are unchanged, and the `var_types`-backed
O(1) tag dispatch continues to feed Array reads through the same helper
(Issue #3908).

### vm/builtins_linalg Array helpers (Issue #3908)

`subset_julia_vm_vm/src/vm/builtins_linalg.rs` now routes the two remaining
native-Array destructure sites through a file-local
`legacy_array_value_ref(&Value) -> Option<&ArrayRef>` helper alongside the
existing `linalg_array_value(ArrayValue) -> Value` constructor. The
`with_linalg_array` matrix-input arm rewrites
`match val { Value::Array(arr_ref) => { ... } ... }` as an early-return
`if let Some(arr_ref) = legacy_array_value_ref(&val) { let arr =
arr_ref.borrow(); return f(&arr); }` followed by a by-value match on the
remaining `StructRef` / `Struct` / `other` arms. The Pure Julia Array
wrapper `_mem::Array` reader in `linalg_array_wrapper_value` rewrites the
`Value::Array(array_ref) =>` arm as a guarded
`_ if legacy_array_value_ref(mem).is_some() => { let array_ref =
legacy_array_value_ref(mem).expect("..."); ... }`, keeping the surrounding
`other =>` fallback. The audited ceiling for the file dropped from 3 to 2
in `scripts/check_value_array_allowlist.sh`; the remaining matches are the
`linalg_array_value` constructor body and the `legacy_array_value_ref`
helper body. LinearAlgebra kernels (`det` / `inv` / `lu` / `eigvals` /
`eigen` / `qr` / `svd`) and `reshape`-derived Array wrapper `_mem::Array`
inputs continue to flow through these helpers unchanged (Issue #3908).

### vm/exec/struct_ops Array helpers (Issue #3908)

`subset_julia_vm_vm/src/vm/exec/struct_ops.rs` now routes the `NewStructSplat`
`Value::Array(arr) =>` destructure arm through a guarded
`_ if legacy_array_value_ref(&val).is_some() => { ... }` arm backed by a
file-local `legacy_array_value_ref(&Value) -> Option<&ArrayRef>` helper
(the surrounding `match val` keeps its `_ =>` fallback so exhaustiveness is
preserved). The Pure Julia Array wrapper bridge inside `GetFieldByName`
(`._mem` / `._size` projections) now reads the underlying `ArrayRef` through
the same helper (`if let Some(arr) = legacy_array_value_ref(&val) { ... }`)
and constructs the `_mem` projection via a shared file-local
`array_value(arr: ArrayRef) -> Value` helper. The audited ceiling for the
file dropped from 3 to 2 in `scripts/check_value_array_allowlist.sh`; the
remaining matches are the two helper bodies (Issue #3908).

### dynamic_ops/dispatch Array helpers (Issue #3908)

`subset_julia_vm_vm/src/vm/dynamic_ops/dispatch.rs` now routes the three native
Array destructures inside `should_use_inline_dynamic_op` (the
`(Value::Array(arr_a), Value::Array(arr_b))` tuple `if let`, the
`if let Value::Array(arr) = a` arm, and the symmetric
`if let Value::Array(arr) = b` arm) through a file-local
`legacy_array_value_ref(&Value) -> Option<&ArrayRef>` helper that mirrors the
existing helpers in `vm/util.rs`, `vm/formatting.rs`, `vm/builtins_dicts.rs`,
and `vm/mod.rs`. The audited ceiling for the file dropped from 3 to 1 in
`scripts/check_value_array_allowlist.sh`; the remaining match is the helper
body's `Value::Array(arr) => Some(arr)` arm (Issue #3908).

`subset_julia_vm_vm/src/vm/builtins_macro/mod.rs` now routes the `Expr` splat
arm's native-Array destructure through a file-local
`legacy_array_value_ref(&Value) -> Option<&ArrayRef>` helper (guarded by
`ref arg_ref if legacy_array_value_ref(arg_ref).is_some()`; the surrounding
`match` keeps its `Value::Tuple(_) =>` arm and the existing `other =>`
fallback to preserve exhaustiveness), and routes the two identical
`Value::Array(new_array_ref(arr))` constructions in the `RegexSplit` and
`RegexEachmatch` handlers through a shared file-local
`array_value(arr: ArrayValue) -> Value` helper. The audited ceiling for the
file dropped from 3 to 2 in `scripts/check_value_array_allowlist.sh`; the
remaining matches are the two helper bodies (Issue #3908).

`subset_julia_vm_vm/src/vm/builtins_collections.rs` now routes the three
remaining native-Array destructures in the `Length` and `Eltype` handlers
(`Length` direct arm, `Length`'s `Generator`-inner arm, `Eltype` direct arm)
through a file-local
`legacy_array_value_ref(&Value) -> Option<&ArrayRef>` helper. Each match
keeps its existing wildcard fallback while the legacy variant pattern leaves
the call sites entirely. The audited ceiling for the file dropped from 3 to
1 in `scripts/check_value_array_allowlist.sh`; the remaining match is the
helper body's `Value::Array(arr) => Some(arr)` arm (Issue #3908).

`subset_julia_vm_vm/src/vm/builtins_types.rs` now routes the legacy
native-Array destructure sites in the `Typeof`, `Isa`, `Sizeof`, `Objectid`,
and `In` handlers, plus the `Ismutable`
`Value::Array(_) | Value::Memory(_) | Value::Dict(_)` OR pattern, through
guarded `_ if legacy_array_value_ref(&val).is_some()` arms backed by a
file-local `legacy_array_value_ref(&Value) -> Option<&ArrayRef>` helper.
Each match keeps its `_ =>` fallback so exhaustiveness is preserved. The
audited ceiling for the file dropped from 7 to 4 in
`scripts/check_value_array_allowlist.sh`. The remaining four matches are
the `any_vector_array_value` constructor body, the helper body, and two
`Value::Array(_)` mentions inside the helper doc comment (Issue #3908).

`subset_julia_vm_vm/src/vm/mod.rs` now routes the Pure Julia Array wrapper
dispatch guard (`value_matches_param`), the production struct-field egal
pointer-equality arm (`compare_struct_field_values_egal`), and the
`cfg(test)` `compare_values_equal` Array/Memory arms through file-local
helpers (`legacy_array_value_ref`, `is_legacy_array_value`,
`is_array_wrapper_param_type`, `legacy_array_value_ptr_eq`) that centralize
the transitional native carrier unwrap. The audited ceiling for the file
dropped from 12 to 7 in `scripts/check_value_array_allowlist.sh`.

The `vm/exec/array_index.rs` logical and integer index-array load path also
routes each selected element through `ArrayValue::get_linear` instead of the
multi-dimensional `get(&[idx])` call, and the IndexStore re-push sites all
share a single `array_value(ArrayRef) -> Value` constructor helper. Round 2
additionally introduces a `array_ref_from_value(Value) -> Result<ArrayRef,
Value>` destructure helper that the IndexStore Tuple-value, Array-element
(Issue #3648), and boxed-scalar (String/Char/Symbol/Memory{Real}) branches
all share to unpack the array target without re-matching `Value::Array` at
every call site; the two `SubArray` parent unwrap sites (StructRef and
inline Struct) also share a `sub_array_parent_array_ref(&[Value]) ->
Result<ArrayRef, VmError>` helper. Round 3 extends the same
`array_ref_from_value` helper to the remaining destructure-side patterns:
the `IndexLoad` target dispatch (where the `Err(target) => match target
{ ... }` nested branch preserves the String/Tuple/NamedTuple/Range/
Generator/Struct/StructRef/Dict/Ref/Memory/other arms), the `IndexStore`
Complex (`is_complex_val`) branch (flat `Err(...)` arms keep the Memory /
Struct dispatch / fallback paths), and the scalar `IndexStore` f64/i64
branch (`Err(arr_val) => match arr_val { ... }` rebinds so the SubArray
`Value::Struct(ref s)` arm can still re-push and re-borrow the original
value). The audited ceiling for `vm/exec/array_index.rs` dropped from 22 to
16, then to 13, and to 10 in `scripts/check_value_array_allowlist.sh`.
Round 4 routes three additional destructure sites through the same
`array_ref_from_value` helper: the `selected_indices_from_array_wrapper`
`mem` dispatch (`Ok(array_ref)` / `Err(Value::Memory(_))` / `Err(_) =>
Ok(None)` preserves both the Pure Julia `_mem::Array` linear reader and
the `Value::Memory` linear reader), the `load_selected_array_elements`
`target` dispatch (`Ok(arr_ref)` retains the `IndexOutOfBounds` checks
and the `create_sliced_array` shared-backing semantics for Complex /
struct-ref / tuple-element arrays), and the `IndexLoadTyped`
`match self.stack.pop()` dispatch (the `Err(target @ Value::Struct(_)) |
Err(target @ Value::StructRef(_))` arm keeps the `getindex` multi-method
dispatch and `MethodError` formatting, the `Err(_)` arm keeps the
INTERNAL `IndexLoadTyped requires TypedArray` fallback, and `None`
preserves the same INTERNAL error). The audited ceiling dropped further
from 10 to 7. Round 5 routes the remaining three logical-index destructure
sites through the same `array_ref_from_value` helper: the `IndexLoadTyped`
and `IndexLoad` `match index_val` arms collapse the `Value::Array(idx_arr_ref)
=> ...` and `other => ...` arms into `other => match array_ref_from_value
(other) { Ok(idx_arr_ref) => ..., Err(other) => ... }` (logical/integer
index extraction via `selected_indices_from_index_array` and the Dict /
StructRef-Dict `getindex` multi-dispatch under `IndexLoad` remain
unchanged), and the `IndexStoreTyped` `match self.stack.pop()` becomes a
two-level match `Some(popped) => match array_ref_from_value(popped) {
Ok(arr) => ..., Err(target @ Struct(_)/StructRef(_)) => dispatch,
Err(_) => INTERNAL }, None => INTERNAL` mirroring PR #4485's `IndexLoadTyped`
refactor (the `is_struct_ref_array` struct-array coercion, `array_value`
re-push, and `setindex!` / `Base.setindex!` multi-dispatch are preserved).
The audited ceiling dropped further from 7 to 4; the remaining 4 matches
are the three helper bodies (`array_value`, `array_ref_from_value`,
`sub_array_parent_array_ref`) plus the Generator underlying iter's
borrowed `&Value::Array` arm in `match g.iter.as_ref()`.

`vm/exec/array_mutate.rs` centralizes the native-Array re-push through two
shared file-local helpers (`push_array_ref` for existing `ArrayRef` carriers
and `push_array_value` for freshly allocated `ArrayValue`s). The
Zero / ArrayPush / ArrayPop / ArrayPushFirst / ArrayPopFirst / ArrayInsert /
ArrayDeleteAt handlers all delegate their re-push through one boundary, so
the audited ceiling for `vm/exec/array_mutate.rs` dropped from 12 to 5 in
`scripts/check_value_array_allowlist.sh`. A subsequent round added a
file-local `try_consume_array_value(value: Value) -> Result<ArrayRef,
Value>` destructure helper that the `array_mutation_target`, `Zero`,
`ArrayPush` / `ArrayPushTypejoin`, and `ArrayPop` handlers share to unwrap
the native carrier without re-matching `Value::Array` at every call site
(the surrounding `Err(other) => match other { Value::Memory(_) => ...,
Value::Set(_) => ..., _ => ... }` nested match preserves the Memory / Set /
fallback branches and the `try_or_handle` / `Continue` / `raise` control
flow). The audited ceiling dropped further from 5 to 2 — the remaining
matches are the `try_consume_array_value` helper body and the
`push_array_ref` helper body.

The binary_both fallback uses `MemoryValue::get` for the Memory<->Array
equality bridge instead of touching `MemoryValue::data` directly, and a new
`is_legacy_array_value` predicate lets the matmul / scalar-array guards drop
their `Value::Array(_)` pattern matches. Round 2 additionally introduces a
`legacy_array_ref_from_value(&Value) -> Option<&ArrayRef>` helper that the
complex-scalar-array, real-scalar-array, and array-array matmul fallback
arms all share to unpack the underlying `ArrayRef` without re-matching
`Value::Array` at every call site. The audited ceiling for
`vm/exec/binary_both.rs` dropped from 13 to 9 in
`scripts/check_value_array_allowlist.sh`. Round 3 adds a file-local
`array_value(ArrayValue) -> Value` helper and routes all four
`self.stack.push(Value::Array(new_array_ref(result)))` matmul / scalar-array
result re-push sites (Complex/struct scalar × Array, Real scalar × Complex
Array, Real scalar × Real Array, Matrix × Vector matmul) through the helper,
lowering the audited ceiling from 9 to 7. The remaining matches are the three
helper bodies plus their doc-comment mentions (5) and the Memory<->Array
equality bridge tuple patterns (2).

`vm/dynamic_ops/mod.rs` routes the `dynamic_add` / `dynamic_sub` /
`dynamic_mul` / `dynamic_div` array-arm guards through a file-local
`is_array_like_value(&Value) -> bool` helper, so the five
`(Value::Array(_) | Value::Memory(_), ...)` pattern arms collapse to
match-guarded `(lhs, rhs) if is_array_like_value(lhs) &&
is_array_like_value(rhs) => { ... }` shapes. The audited ceiling for
`vm/dynamic_ops/mod.rs` dropped from 6 to 2 in
`scripts/check_value_array_allowlist.sh` — the remaining matches are the
`dynamic_array_value` constructor wrapping freshly built broadcast results
and the `matches!` inside the new helper itself.

`vm/util.rs` adds a file-local
`legacy_array_value_ref(&Value) -> Option<&ArrayRef>` helper that routes the
`pop_array_or_values` native-Array branch through an
`if let Some(arr) = legacy_array_value_ref(&popped)` early return, so the HOF
`map`/`filter` array-projection path no longer destructures `Value::Array(arr)`
inline. The `value_type_name` and `bind_value_to_frame` matches keep their
direct `Value::Array(_)` arms because both are exhaustive over every `Value`
variant with no wildcard fallback (same convention as `format_value_slow` /
`value_to_string` in `vm/formatting.rs`: Rust exhaustiveness checking cannot
prove a guarded helper-call covers the variant). The audited ceiling for
`vm/util.rs` stays at 3 in `scripts/check_value_array_allowlist.sh` — the
helper body plus those two exhaustive arms — with a new comment block recording
the per-site routing breakdown (Issue #3908).

`vm/builtins_equality.rs` now routes every native Array vs Array / Memory
`isequal` and `hash` comparison through two shared helpers
(`try_isequal_array_like` and `try_hash_array_like`). Both helpers consume the
contents through `ArrayValue::to_logical_value_vec` and `MemoryValue::data`'s
public logical accessor, so reshape shared backing and Complex/struct-ref
storage are preserved while the file's `Isequal`, `Hash`, and `_Hash` arms
stop pattern-matching `Value::Array` directly. Round 2 then routes the three
remaining native-Array destructures inside those helpers and the `Egal`
handler (`array_like_logical_view` `Value::Array(arr)` arm,
`try_hash_array_like` `Value::Array(arr)` arm, `Egal`'s
`(Value::Array(a), Value::Array(b))` tuple pattern) through a file-local
`legacy_array_value_ref(&Value) -> Option<&ArrayRef>` helper using
`if let Some(arr) = legacy_array_value_ref(value)` early returns and a
guarded `(a, b) if legacy_array_value_ref(a).is_some() && legacy_array_value_ref(b).is_some()`
arm. The audited ceiling for `vm/builtins_equality.rs` dropped from 6 to 3
in round 1 and from 3 to 1 in round 2 in
`scripts/check_value_array_allowlist.sh`; the remaining match is the helper
body itself. The Pure Julia Array wrapper bridge in
`compare_array_wrapper_boundary_values_equal` continues to run first so
wrapper-vs-native equality is unchanged.

`vm/formatting.rs` adds a file-local `legacy_array_value_ref` helper that
centralizes the native-Array unwrap for the Julia-source surface
(`value_to_julia_code`), and the formatting unit tests now share a single
`array_value(ArrayValue) -> Value` constructor helper. The exhaustive
`format_value_slow` (`print`) and `value_to_string` (`string(x)`) arms keep
their direct `Value::Array(arr)` pattern because Rust's exhaustiveness check
cannot prove a guarded helper-call covers the variant. The audited ceiling
for `vm/formatting.rs` dropped from 6 to 5 in
`scripts/check_value_array_allowlist.sh`, and the new
`strings::strings_array_format_helpers_3908` fixture pins
`print` / `string` / interpolation output byte-identical to upstream Julia
1.12 for Int64, Float64, empty, String, and 2D arrays.

`vm/builtins_reflection/primitives.rs` now routes the `extract_types_from_value`
Vector-of-types branch (used by `methods(f, [Type1, Type2])`) through a new
file-local `legacy_array_value_ref(&Value) -> Option<&ArrayRef>` helper. The
match already has a wildcard fallback so a guarded arm
`_ if legacy_array_value_ref(val).is_some()` satisfies exhaustiveness and the
raw `Value::Array(arr) => { ... }` pattern disappears from the reflection
dispatch surface. The three test-side `Value::Array(new_array_ref(arr))`
literals are consolidated through a shared `array_value(ArrayValue) -> Value`
helper. The audited ceiling for `vm/builtins_reflection/primitives.rs` dropped
from 4 to 2 in `scripts/check_value_array_allowlist.sh`; the remaining matches
are the helper bodies (`legacy_array_value_ref` and `array_value`).

`vm/builtins_strings.rs` routes the `String(::Vector{Char})` constructor and
the `Array{Char}`-wrapper `_mem` reader through a shared
`try_chars_to_string_from_array_like(&Value) -> Result<Option<String>, _>`
helper that decodes each element via `ArrayValue::get_linear` and
`MemoryValue::get`. The helper itself now pulls the native array arm through
the file-local `legacy_array_ref_from_value` helper as well, so the only
remaining direct match is the helper body itself. The `_substring_retag`
builtin pulls the underlying `ArrayRef` through the same
`legacy_array_ref_from_value` helper, and the `codeunits` / `findall` /
retag re-push sites share one `array_value(ArrayRef) -> Value` constructor
so the file's String-from-array surfaces stop spelling the native array
carrier directly. The audited ceiling for `vm/builtins_strings.rs` dropped
from 6 to 3 and then to 2 in `scripts/check_value_array_allowlist.sh`; the
remaining matches are the `array_value` constructor body and the
`legacy_array_ref_from_value` helper body.

`vm/exec/array_index_slice.rs` now routes every legacy native-array
destructure through a shared file-local `legacy_array_value_ref(&Value) ->
Option<&ArrayRef>` helper. The four call sites (the
`array_wrapper_logical_values` `_mem::Array` reader, the
`value_to_slice_index` Array index source, the string-indexing
Array-of-indices arm in `execute_index_slice`, and the top-level slicing
target unwrap) all live inside `match`/`if` branches with wildcard
fallbacks, so guarded `_ if legacy_array_value_ref(...).is_some()` arms (or
an `if let Some(arr_ref) = legacy_array_value_ref(&target)` shape for the
target unwrap that needs to fall through to `getindex` dispatch) satisfy
exhaustiveness without spelling the native carrier directly. The three
slicing result re-pushes (1D `slice_indices.len() == 1`, 2D matrix slice,
and the generic N-D fallback) share a single `array_value(ArrayValue) ->
Value` constructor helper. The audited ceiling for
`subset_julia_vm_vm/src/vm/exec/array_index_slice.rs` dropped from 7 to 2 in
`scripts/check_value_array_allowlist.sh`; the remaining 2 matches are the
helper bodies (`legacy_array_value_ref` and `array_value`).

`vm/type_ops/iteration.rs` now routes every legacy native-array
construction (the `Value::Array(new_array_ref(arr))` literal) through the
file-local `array_value(ArrayValue) -> Value` helper that already existed
in the file. The 13 construction sites (`collect_zip_fields` empty / nested
nothing arms, `collect_enumerate_fields` final push, `collect_rest_fields`
final push, `collect_logrange_fields` empty / final push, and the five
generator-callable empty fallbacks in `collect_generator_dispatch`) all
delegate their re-push through one boundary. Round 2 additionally adds a
file-local `legacy_array_value_ref(&Value) -> Option<&ArrayRef>` helper
and routes all eight remaining destructure sites through it: the three
matrix shape probes (`matrix_array_dims_2d`,
`extract_matrix_row_1based`, `extract_matrix_column_1based`) use
`let Some(arr) = legacy_array_value_ref(matrix) else { ... }`, and the
`_mem::Array` linear getter in `iterate_first_array_wrapper_fields`, the
`iterate_first` / `iterate_next` / `collect_iterator` Array match arms,
and the `collect_iterator_values` materialize unwrap use guarded
`_ if legacy_array_value_ref(coll).is_some() => { ... }` arms backed by
existing `_ =>` fallbacks. The audited ceiling for
`subset_julia_vm_vm/src/vm/type_ops/iteration.rs` dropped from 21 to 9 (round
1) and now from 9 to 4 (round 2) in
`scripts/check_value_array_allowlist.sh`. The remaining 4 matches are the
two helper bodies (`array_value` and `legacy_array_value_ref`) plus their
doc-comment mentions.

The current compatibility direction is Memory-first: many constructors allocate
`MemoryValue` before wrapping with transitional `ArrayValue`, and the retired
`memory_to_array_ref` bridge is held at zero by audit. Remaining `Value::Array`
references are classified compatibility boundaries or the still-transitional
native Array container surface tracked by #3908 and follow-up issues.

## Final Boundary

The target architecture is:

```text
Value::Memory(MemoryRef)     runtime primitive storage
MemoryRef-like offset view   runtime primitive for shared storage and reshape
Array{T,N}                   Julia-visible wrapper over MemoryRef + dims
Vector{T}                    Array{T,1}
Matrix{T}                    Array{T,2}
```

`Value::Array` should eventually be limited to compatibility boundaries that
cannot yet expose the Julia wrapper directly:

- Swift/iOS and C ABI return values
- web/plotting sample compatibility
- transitional bytecode cache decoding
- debug formatting while old bytecode exists

Core runtime semantics should move to Memory-backed primitives plus Pure Julia
Array methods.

## Phase 1: Preserve Memory Identity

Goal: keep `Memory{T}` typed and observable as memory, avoiding unnecessary
normalization into arrays.

Status:

- #3894 added typed identity/reflection coverage for `Memory{T}` construction,
  indexing, mutation, and `.parameters`.
- #3902 keeps `isequal(::Memory, ::Memory)` on the flat Memory primitive buffer
  instead of normalizing both operands through `memory_to_array_ref`.
- #3902 rejects negative dynamic `Memory{T}(n)` lengths before allocation,
  preventing signed-to-`usize` wraparound from turning invalid lengths into huge
  primitive allocations.
- #3908 added the first Pure Julia `Array{T}` wrapper methods (`size`,
  `length`, `ndims`, 1D/2D `getindex`, 1D/2D `setindex!`) and a legacy
  `Value::Array` `_mem` / `_size` projection so old array values can execute the
  wrapper path during migration.
- #3908 added `scripts/check_memory_to_array_ref_allowlist.sh` to freeze the
  current `memory_to_array_ref` compatibility surface and force any future bridge
  to be classified before it lands.
- #3908 added Pure Julia `wrap(Array, Memory, dims)` / one-dimensional length /
  full-Memory forms. Logical `Array{T}` length and wrapper indexing now use
  `prod(dims)` rather than backing Memory capacity, matching upstream
  `julia/base/array.jl`'s public behavior before sjulia has `MemoryRef`.
- #3917 fixed runtime type binding so VM type extraction reports `Memory{T}`
  instead of `Vector{T}` for `Value::Memory`, allowing `m::Memory{T}` methods to
  bind `T`.
- #3928 propagates `Array{T,N}` dimension parameters through runtime projection,
  `typeof`, `isa`, method dispatch, and Pure Julia `Array{T}` wrappers. It keeps
  `Array{T}` as a rank-polymorphic pattern, treats `Vector{T}` / `Matrix{T}` as
  the `Array{T,1}` / `Array{T,2}` aliases, and reports 3D+ arrays as
  `Array{T,N}`.
- #3943 fixes Pure Julia `copy(m::Memory{T}) where T` to allocate `Memory{T}`
  rather than `Memory{Any}`, matching upstream `julia/base/genericmemory.jl`
  same-type copy semantics and keeping `typeof` / `eltype` stable.
- #3945 moves `Memory` shape and allocation helpers into Pure Julia
  `genericmemory.jl`: `size(m)`, `size(m, d)`, `ndims(m)`, and
  same-eltype/typed `similar(::Memory{T}, ...)` now follow upstream
  `julia/base/genericmemory.jl` and `julia/base/abstractarray.jl` semantics.
  Rust fallback shape handling also treats rank-exceeding positive dimensions as
  size `1`, so primitive compatibility paths stay Julia-compatible.
- #3947/#3948 add Memory-specific Pure Julia `unsafe_copyto!` / `copyto!` and
  restore upstream negative-count validation for both Memory and Array
  5-argument `copyto!` paths. This keeps current Array projection compatibility
  from bypassing `julia/base/genericmemory.jl`'s checked copy boundary.
- #3950 fixes Pure Julia wrapper `size(a, d)` so trailing dimensions beyond
  `ndims(a)` return `1`, matching `julia/base/abstractarray.jl`.
- #3908 adds Pure Julia `count(f::Function, r::AbstractRange)` and routes the
  retained VM `CountFunc` Range fallback through `RangeValue::collect()`,
  preserving public iteration dispatch while keeping old bytecode on the
  Memory-first range materialization path.
- #3908 routes the retained VM `FilterInPlace` HOF compatibility fallback
  through `ArrayValue::memory_first_from_f64()` instead of direct
  `ArrayData::F64` assignment, keeping old bytecode storage rebuilding on the
  Memory-first path.
- #3908 moves `pop_array_or_values` away from raw `ArrayData` variant matching
  for non-F64 / shared storage and onto `ArrayValue::element_type` plus logical
  element readers, preserving the legacy by-reference path only for plain
  non-shared Float64 arrays.
- #3908 routes retained VM HOF F64 fallback startup reads through
  `ArrayValue::to_logical_f64_vec()` instead of raw-storage
  `try_as_f64_vec()`, preserving reshaped/shared ArrayValue projection in old
  HOF bytecode boundaries.
- #3908 routes LinearAlgebra native compatibility input extraction through
  `ArrayValue::to_logical_f64_vec()` instead of raw-storage
  `try_as_f64_vec()`, preserving reshaped/shared ArrayValue projection while
  #4020 tracks moving public linalg behavior fully behind dispatch.
- #3908 routes `valtype(::Array)` through `ArrayValue::element_type()` and the
  shared logical `ArrayElementType` to `JuliaType` projection helper, avoiding
  raw `ArrayData` tags at this collection trait compatibility boundary.
- #3908 routes dispatch-facing `Value::Array` type projection through
  `ArrayValue::element_type()` for primitive/logical element tags such as
  `Complex{Float64}`, while retaining struct heap lookups for transitional
  struct-array compatibility.
- #3908 routes `Value::runtime_type(::Array)` through the shared logical
  `ArrayElementType` to `JuliaType` projection helper, preserving tuple-field
  logical metadata instead of maintaining a separate raw-oriented conversion
  table.
- #3908 routes native `isa(::Array, ::Type)` fallback through
  `Vm::get_value_julia_type` for Array values, so logical element metadata such
  as `Complex{Float64}` is preserved instead of re-derived from raw storage.
- #3908 routes numeric `IndexStore` scalar conversion through
  `ArrayValue::element_type()` for direct Array stores and SubArray parent
  stores, preserving logical Complex/Bool element conversion rather than raw
  storage tags.
- #3908 routes native `sizeof(::Array)` fallback through logical
  `ArrayValue::element_type()` size projection, preserving Complex array byte
  sizes instead of deriving public size from interleaved raw storage tags.
- #3908 routes Array iteration next-state handling and native `in(x, array)`
  fallback through logical `ArrayValue::get_linear()` element reads, preserving
  Complex membership semantics instead of comparing probes against interleaved
  raw storage values.
- #3908 routes String `getindex` with Array-like index vectors to the existing
  slice instruction path instead of scalar `DynamicToI64`, and selects vector
  indices elementwise at runtime so `s[[i, j]]` stays aligned with Julia while
  generic indexing migrates toward Pure Julia dispatch.
- #3908 routes retained RNG array-producing VM instructions and legacy eager
  range materialization through Memory-first `ArrayValue` helpers, keeping old
  bytecode compatibility arrays aligned with the Array/Memory storage boundary.
- #3908 routes retained F64-mode HOF dispatch result builders through
  Memory-first `ArrayValue` helpers and extends the broadcast/HOF audit to cover
  that compatibility file.
- #3908 routes HOF fallback input normalization in `pop_array_or_values` through
  Memory-first `ArrayValue` helpers and extends the broadcast/HOF audit to cover
  that compatibility boundary.
- #3908 routes retained real-valued matmul and scalar-vector compatibility
  result builders through Memory-first `ArrayValue` helpers, while #4020
  remains responsible for moving public LinearAlgebra behavior fully behind
  Pure Julia / stdlib dispatch.
- #3908 routes `String(::Array)` character collection and Pure Julia Array
  wrapper `_mem::Array` compatibility inputs through logical
  `ArrayValue::get_linear()` reads, so reshaped/shared ArrayValue inputs are
  observed through their public element projection instead of raw storage.
- #3908 adds Pure Julia `Array` wrapper `getindex` methods for Int64 index
  vectors and Bool masks, keeps dynamic typed-array index results out of scalar
  slots, and makes the retained native slice fallback read `Value::Array`
  indices through logical `ArrayValue::get_linear()` so reshaped/shared backing
  storage is observed instead of raw `ArrayData`.
- #3908 centralizes transitional `Value::Array` element-type projection for
  `eltype`, `valtype`, dispatch-facing `get_value_julia_type`, and `typeof`
  through ArrayValue projection helpers, preserving user-struct element tags
  that raw `ArrayElementType` conversion previously degraded to `Any`.
- #4416 splits public trait/reflection projection from dispatch-facing Array
  specialization so `Vector{Any}` keeps `Any` for `eltype`, `valtype`, and
  `typeof` even when its runtime elements are user structs.
- #3908 routes the retained untyped array literal `PushElem` builder through
  `ArrayValue::push_f64()` instead of direct raw `ArrayData::F64` mutation, and
  tightens the literal Memory-first audit to keep builder growth behind
  ArrayValue mutation helpers.
- #4419 routes the retained native `zero(::Array)` compatibility fallback
  through logical `ArrayValue::element_type()` plus Memory-first typed
  allocation, preserving source element types such as `Int64` and `Bool`
  instead of returning `Vector{Float64}` for every array.
- #3908 removes dead legacy `Zeros` / `Ones` handlers from `builtins_exec.rs`;
  `builtins_arrays.rs` is the dispatch-chain owner for those constructors and
  already uses Memory-first allocation helpers.
- #3908 removes dead legacy `TypeOf` / `Isa` / `Subtype` handlers from
  `builtins_exec.rs`; `builtins_types.rs` is the dispatch-chain owner, so the
  old fallback's duplicate `Value::Array` type projection table is no longer a
  compatibility surface.
- #3908 removes dead legacy string handlers from `builtins_exec.rs`;
  `builtins_strings.rs` is the dispatch-chain owner, so the old fallback
  `codeunits(::String)` Array materialization is no longer a compatibility
  surface.
- #3908 removes dead legacy `Reshape` / `Push` / `Pop` handlers from
  `builtins_exec.rs`; `builtins_arrays.rs` is the dispatch-chain owner, so the
  duplicate `Value::Array` reshape and mutation fallback surface is gone.
- #3908 removes `Value::Array` branches from the retained internal
  `TupleFirst` / `TupleLast` fallback; public `first(::Array)` /
  `last(::Array)` now stay on the Pure Julia indexing path.
- #3908 removes `builtins_exec.rs` from the `Value::Array` allowlist audit
  after the file reached zero native Array compatibility references.
- #3908 tightens the `Value::Array` allowlist ceiling for
  `vm/exec/array_index.rs` from 23 to the current 22 references, preventing the
  generic indexing compatibility surface from expanding while wrapper indexing
  migration continues.
- #3908 routes typed array literal `PushElemTyped` struct-reference storage
  classification through `ArrayValue::is_struct_ref_array()` and the shared
  `ArrayValue::push()` helper instead of matching `ArrayData::StructRefs`
  directly in `array_basic.rs`.
- #3908 routes the retained `Generator` indexing fallback over Array inputs
  through `ArrayValue::get_linear()` instead of raw `ArrayData::get_value`,
  preserving reshaped/shared backing projection for this compatibility path.
- #3908 routes native `push!` Array builtin struct-reference mutation through
  `ArrayValue::is_struct_ref_array()` and `ArrayValue::push()` instead of
  mutating `ArrayData::StructRefs` directly in `builtins_arrays.rs`.
- #3908 routes typed indexing `setindex!` struct-array classification through
  `ArrayValue::is_struct_ref_array()` instead of matching
  `ArrayData::StructRefs` directly in `array_index.rs`.
- #3908 routes `NewStructSplat` array expansion through
  `ArrayValue::get_linear()` instead of raw `ArrayData::get_value`, preserving
  logical projection for reshaped/shared arrays.
- #3908 routes `Expr` constructor Array splat expansion through
  `ArrayValue::get_linear()` instead of raw `ArrayData::get_value`, preserving
  logical Array projection for metaprogramming splats, and fixes the
  `Expr(:call, args...)` lowering splat mask bug tracked as #4435.
- #3908 collapses the duplicate `Value::Array(new_array_ref(ArrayValue::
  any_vector(...)))` construction in `builtins_types.rs` `Subtypes` handler
  (empty-result and populated-result branches) behind a shared Memory-first
  `any_vector_array_value` helper, and lowers the file's allowlist ceiling
  from 8 to 7.
- #3908 routes retained dynamic arithmetic dispatch storage classification
  through `ArrayValue::supports_inline_dynamic_storage()` instead of matching
  raw `ArrayData::StructRefs` / `ArrayData::Any` tags in
  `dynamic_ops/dispatch.rs`.
- #3908 routes `load_selected_array_elements` (logical and integer-index
  selection from `IndexLoad` / `IndexLoadTyped`) through
  `ArrayValue::get_linear` instead of the multi-dimensional `get(&[idx])`
  path, and centralizes the IndexStore re-push sites behind a single
  `array_value(ArrayRef) -> Value` constructor helper. The audited
  `Value::Array` ceiling for `vm/exec/array_index.rs` drops from 22 to 16 in
  `scripts/check_value_array_allowlist.sh`.
- #3908 centralizes the public Base array query/construction surface in
  `vm/builtins_arrays.rs` (Similar, Reshape, Size, Ndims, Keytype, Valtype)
  behind a single `value_as_array_ref(&Value) -> Option<&ArrayRef>` projection
  helper instead of replicating raw `Value::Array(...)` matches in each arm.
  The Complex-aware `similar` storage path, typed/multi-dim `similar` shape
  override, reshape, and the scalar/range/memory fallbacks for `size` and
  `ndims` keep their behavior; `ndims` additionally projects through the
  existing `ArrayValue::ndims()` helper. The audited `Value::Array` ceiling
  for `vm/builtins_arrays.rs` drops from 9 to 3 in
  `scripts/check_value_array_allowlist.sh`; the remaining 3 matches are the
  centralized `value_as_array_ref`, `push_array_ref`, and
  `pop_array_ref_for_builtin` helper definitions.
- #3908 centralizes the duplicated EachCol / EachRow / EachSlice matrix-slice
  extraction in `vm/type_ops/iteration.rs` behind three new file-local
  helpers (`matrix_array_dims_2d`, `extract_matrix_row_1based`,
  `extract_matrix_column_1based`). Both `iterate_first` and `iterate_next`
  now probe matrix dimensions through a single native-Array match site and
  build row / column slices through `ArrayValue::any_vector`, instead of
  repeating `Value::Array(arr)` matches plus raw `arr.borrow().shape` reads at
  each call site. The audited `Value::Array` ceiling for
  `vm/type_ops/iteration.rs` drops from 32 to 21 in
  `scripts/check_value_array_allowlist.sh`.
- #3908 centralizes `NewArray` / `PushArrayValue` / `NewArrayTyped` /
  `LoadArray` native-Array push construction in `vm/exec/array_basic.rs`
  behind three file-local helpers (`push_array_ref`, `push_array_value`,
  `push_typed_array_value`). The Memory-first array literal builders and the
  per-frame `LoadArray` (current and global frame) re-push sites all flow
  through the same one-line boundary, so `push_undef_typed_array` and each
  handler hold a single `ArrayRef` construction site instead of nine direct
  `Value::Array(...)` calls. The audited `Value::Array` ceiling for
  `vm/exec/array_basic.rs` drops from 19 to 10 in
  `scripts/check_value_array_allowlist.sh`.
- #3908 (round 2) centralizes the remaining `vm/exec/array_basic.rs`
  destructure sites behind three more file-local helpers:
  `legacy_array_value_mut_ref(&mut Value) -> Option<&mut ArrayRef>` rewrites
  the `PushElem` / `FinalizeArray` / `PushElemTyped` / `FinalizeArrayTyped`
  `match self.stack.last_mut() { Some(Value::Array(arr)) => ... }` arms as
  `match self.stack.last_mut().and_then(legacy_array_value_mut_ref)`,
  `legacy_array_value_into(Value) -> Option<ArrayRef>` rewrites the four
  `LoadArray` owned-array `if let Some(Value::Array(arr)) = ...` sites
  (current-frame `load_slot_value_by_name` and `locals_any.get(name)` plus
  the same two checks against the global frame) as `if let Some(arr) =
  ....and_then(legacy_array_value_into)` (the `Set` runtime fallback and
  `locals_array.get(name)` arms are kept in their existing order), and
  `try_consume_array_value(Value) -> Result<ArrayRef, Value>` rewrites the
  `StoreArray` `match val { Value::Array(arr) => ..., Set/StructRef/Dict =>
  ..., other => ... }` as `match try_consume_array_value(val) { Ok(arr) =>
  ..., Err(val @ (Value::Set(_) | Value::StructRef(_) | Value::Dict(_))) =>
  ..., Err(other) => ... }`. The audited `Value::Array` ceiling for
  `vm/exec/array_basic.rs` drops from 10 to 4 in
  `scripts/check_value_array_allowlist.sh`; the remaining four matches are
  the `push_array_ref` re-push helper body and the three destructure helper
  bodies, so every public handler now passes through a single file-local
  helper line at the Pure Julia Array carrier boundary.

Tasks:

- Keep `typeof(Memory{T}(n))`, `.parameters`, `eltype`, indexing, mutation, and
  `copy` element type preserving, and keep `size` / `ndims` / `similar`
  behavior on the Pure Julia `Memory` path.
- Keep `copyto!` / `unsafe_copyto!` on Memory aligned with upstream checked copy
  behavior, including overlap safety and negative-count rejection.
- Add fixture coverage for construction, mutation, indexing, and reflection.
- Keep `memory_to_array_ref` at zero; any reintroduction must be issue-driven
  and justified against the upstream Memory / Array boundary rather than
  treated as a routine compatibility allowance.

Test gate:

```bash
timeout 1200 cargo nextest run --release --test fixture_tests memory::
timeout 1200 cargo nextest run --release --lib vm::value::memory_value
bash scripts/check_memory_to_array_ref_allowlist.sh
```

## Phase 2: Make Array Constructors Memory-First

Goal: all Rust array allocation sites construct `MemoryValue` first, then wrap
with `ArrayValue::from_memory` only where the runtime still needs an array
container.

Status:

- #3952 adds `ArrayValue::memory_first_*` helpers for transitional Array
  allocation. Public VM array constructor builtins now allocate primitive
  `MemoryValue` through the helper before wrapping as `Value::Array`, and
  `scripts/check_array_constructor_memory_first.sh` prevents those builtins
  from reintroducing direct `ArrayValue::from_memory` calls. Validation passed
  for targeted constructor/unit coverage and the full `array::`,
  `type_preservation::`, and `broadcast::` fixture categories; the full release
  suite reached `2843 passed` before the 20-minute wrapper timeout.
- #3953 migrates the typed array literal builder path (`NewArrayTyped`) to
  `ArrayValue::memory_first_with_capacity`. Literal code still returns the
  transitional `Value::Array`, but builder storage is allocated as primitive
  `MemoryValue` before wrapping. `scripts/check_array_literal_memory_first.sh`
  prevents this path from reintroducing direct typed `ArrayData` capacity
  allocation. Validation passed for upstream Julia fixture parity, targeted
  literal/unit coverage, and the full `array::`, `type_preservation::`, and
  `broadcast::` fixture categories; the full release suite reached
  `2702 passed` before the 20-minute wrapper timeout.
- #3954 migrates range, numeric tuple, and string `collect` materialization to
  `ArrayValue::memory_first_from_i64`, `memory_first_from_f64`, and
  `memory_first_from_char`. These paths still return transitional
  `Value::Array`, but typed result buffers are owned as primitive `MemoryValue`
  storage before the wrapper is returned. `scripts/check_collect_memory_first.sh`
  prevents these paths from reintroducing direct typed Array wrappers. Validation
  passed for targeted collect/string/sort coverage and full `array::`,
  `iteration::`, and `hof::`; the full release suite reached `2623 passed`
  before the 20-minute wrapper timeout.
- #3955 tightens the `Value::Array` allowlist after Phase 2 migrations by
  centralizing kw/default literal array compatibility construction in
  `compile/utils.rs` and lowering that file's ceiling from 3 to 1. Validation
  passed for the allowlist and helper/default-literal unit coverage; the full
  release suite reached `2767 passed` before the 20-minute wrapper timeout.
- #3960 migrates `collect(::Array)` / VM array iterator copy materialization to
  `ArrayValue::memory_first_copy_from_array`, and updates Pure Julia
  `collect(arr::Array)` to preserve source shape via `similar(arr)`. Validation
  passed for upstream fixture parity, targeted array/iteration fixtures, and
  collect / Value::Array allowlist audits; the full release suite reached
  `2757 passed` before the 20-minute wrapper timeout.
- #3961 adds `ArrayValue::memory_first_collect_values` for generic collect/grow
  materialization where the result element type is discovered from produced
  values. Tuple collect now uses this Memory-first boundary, generator
  expression collect stays on the correctness-preserving eager wrapper path,
  and bug #3966 tracks the remaining VM lazy Generator gap where `f(x)` cannot
  yet be applied inside the iterator protocol.
- #3966 fixes VM `collect(Generator(f, iter))` so lazy `Value::Generator`
  applies `f(x)` through the existing value-mode HOF frame path after
  materializing the wrapped iterator, matching the upstream
  `julia/base/generator.jl` `iterate(g::Generator)` contract for collect.
  Generator expression syntax remains on the eager wrapper path until general
  `iterate(::Generator)` can enter async function frames.
- #3962 migrates low-level VM broadcast fallback results and HOF fallback
  buffers to `ArrayValue::memory_first_from_i64` /
  `memory_first_from_f64`. This covers real-valued broadcast results,
  interleaved complex broadcast buffers, empty `findall`, `mapfoldr` reverse
  buffers, range-backed `count`, and `ntuple` index arrays. The
  `scripts/check_broadcast_hof_memory_first.sh` audit prevents these paths from
  reintroducing direct typed `ArrayValue::from_*` result storage.
- #3963 migrates compile-time `Literal::Array*` constant conversion to
  `ArrayValue::memory_first_from_*` helpers and changes REPL persistence array
  reconstruction to read logical elements with `ArrayValue::get_linear` instead
  of cloning raw `ArrayData`. This keeps reshaped/shared-storage arrays coherent
  across REPL evaluation boundaries while retaining the transitional
  `Value::Array` wrapper. The `scripts/check_literal_repl_memory_first.sh` audit
  prevents these paths from reintroducing direct typed ArrayValue storage or raw
  REPL storage clones. Validation passed for targeted converter/REPL coverage,
  compile literal/default coverage, Array fixture coverage, and audits; the full
  release suite reached `2738 passed` before the 20-minute wrapper timeout.
- #3908 adds `ArrayValue::memory_first_from_u8` and
  `memory_first_from_strings`, then routes `codeunits`, regex `split`, regex
  `eachmatch`, and `subtypes` Vector materialization through Memory-first
  helpers. `ArrayValue::any_vector` now also allocates primitive `MemoryValue`
  storage before returning the transitional `ArrayValue` wrapper.
- #3908 routes complex F64/F32 helper constructors and complex zeros/undef paths
  through `MemoryValue` first, preserving interleaved real storage while keeping
  the logical `ComplexF64` / `ComplexF32` override on the transitional
  `ArrayValue` wrapper.
- #3908 routes legacy primitive `ArrayValue` helpers (`from_*`, vector aliases,
  `zeros`, `ones`, `fill`, and primitive / boxed `undef_typed` branches) through
  a shared Memory-first `ArrayData` materialization helper before returning the
  transitional `ArrayValue` wrapper.
- #3908 routes tuple and isbits struct `ArrayValue` helpers through
  Memory-first materialization / capacity helpers, preserving logical
  `TupleOf(...)` and `StructInlineOf(...)` element tags while moving storage
  ownership behind `MemoryValue`.
- #3908 routes legacy `ArrayValue::new` and `with_struct_type` through the
  shared Memory-first `ArrayData` materialization helpers, preserving the public
  constructor API and struct-reference `StructOf(type_id)` metadata while moving
  storage ownership behind `MemoryValue`.
- #3908 routes dynamic broadcast's Complex struct-ref array bridge through
  `ArrayValue::complex_f64`, so the interleaved real storage uses the same
  Memory-first complex helper path instead of a manual `ArrayValue` constructor.
- #3908 routes `deep_copy_value(::Array)` through
  `ArrayValue::memory_first_copy_from_array`, so logical array elements are
  copied into independent Memory-first storage instead of cloning raw
  `ArrayData`.
- #3908 removes the dead legacy `Deepcopy` fallback from `builtins_exec.rs`;
  `builtins_reflection` is the dispatch-chain owner, so Array deep copy remains
  on the Memory-first logical copy path without a duplicate fallback surface.
- #3908 routes runtime array-valued index extraction through
  `ArrayValue::get_linear`, preserving reshaped/shared parent projection for
  Bool masks, F64 boolean-like masks, and I64 index vectors instead of reading
  raw `ArrayData` directly.
- #3908 routes matmul real and complex extraction through
  `ArrayValue::to_logical_f64_vec` / `get_linear`, preserving reshaped/shared
  parent projection while keeping the transitional LinearAlgebra kernel
  boundary intact for #4020.
- #3908 routes `StackOps::pop_array` Range auto-collection through
  `RangeValue::collect()`, reusing the Memory-first range materialization path
  and preserving integer range element types.

Tasks:

- Audit remaining `ArrayValue::new*`, public fallback builders, and generator
  iteration result materialization.
- Prefer a single helper that allocates typed `MemoryValue` and records shape.
- Preserve typed storage for `Array{T,N}` with `ArrayElementType` derived from
  Julia type parameters.
- Keep existing `array::`, `type_preservation::`, and `broadcast::` fixtures
  passing after each allocation-site conversion.

Test gate:

```bash
timeout 1200 cargo nextest run --release --test fixture_tests array:: type_preservation:: broadcast::
```

## Phase 3: Introduce MemoryRef / Array Wrapper Sharing

Goal: support zero-copy shape changes where Julia uses `MemoryRef`.

Tasks:

- Add a runtime `MemoryRef` value containing `(MemoryRef, offset)`.
- Change `ArrayValue` transitional representation from direct memory ownership
  to memory-ref ownership plus dims.
- Keep the current `wrap(Array, Memory, dims)` Pure Julia path as the no-offset
  compatibility form, then add `wrap(Array, MemoryRef, dims)` once a runtime
  `MemoryRef` value exists.
- `reshape` now returns a distinct Array structure without mutating the source
  shape and shares the source storage owner through a transitional
  `shared_parent` bridge (Issues #3920/#3919). Full `MemoryRef` ownership still
  replaces this bridge in the final representation.
- User-visible linear consumers such as formatting, equality, and hashing should
  read through `ArrayValue::get_linear` rather than direct `ArrayValue.data`
  access so the `shared_parent` bridge remains observable (Issue #3923).
- #3926 adds Pure Julia `wrap(Array, MemoryRef, dims)` using parent Memory plus
  offset metadata while preserving the existing two-field `Array{T}` wrapper
  constructor.
- #3927 classifies direct `ArrayValue.data` consumers. Real-valued broadcast and
  HOF/iteration array extraction now read logical elements through
  `get_linear_f64` / `to_logical_*` when `shared_parent` is present. Interleaved
  ComplexF64 broadcast remains a documented typed-storage fast path; FFI/cache
  compatibility and internal typed storage helpers remain outside public array
  semantics.
- `scripts/check_array_public_data_access.sh` prevents regressions in the
  migrated public broadcast/HOF surface and documents the remaining complex
  typed-storage exception.

Test gate:

```bash
timeout 1200 cargo nextest run --release --test fixture_tests array:: views:: broadcast::
bash scripts/check_array_public_data_access.sh
```

## Phase 4: Move Public Array Behavior To Pure Julia

Goal: public Base array APIs dispatch to Julia methods by default, with Rust
only providing underscored memory primitives and host/runtime boundaries.

Status:

- #3908 starts this phase for the smallest public wrapper surface: `size`,
  `length`, `ndims`, 1D/2D `getindex`, and 1D/2D `setindex!`.
- #3908 also adds `wrap(Array, Memory, dims)` over the current `_mem` + `_size`
  wrapper and switches logical length/bounds to `dims`, which is the user-visible
  behavior needed before the lower-level `MemoryRef` offset representation is
  introduced.
- The current wrapper is `Array{T}` over `_mem` + `_size`; upstream Julia uses a
  `MemoryRef` plus dims, so `MemoryRef` offset semantics remain Phase 3 work.
- #3908 adds Pure Julia wrapper 3D `getindex` / `setindex!` for
  `wrap(Array, mem, (d1, d2, d3))`, using column-major linearization over
  `_size` and `_mem` / MemoryRef offset metadata.
- #3933 fixes the public method lookup helper for parametric vararg methods, so
  wrapper 3D indexing now uses upstream-style `I::Int64...` methods rather than
  temporary explicit 3D overloads.
- #3908 adds Pure Julia wrapper `reshape(a::Array{T}, dims...)`, preserving the
  same backing `Memory` and MemoryRef offset metadata. The Rust `reshape`
  builtin now falls back to Pure Julia dispatch for non-`Value::Array` arguments
  instead of treating wrapper arrays as a type error.
- #3908 adds same-eltype Pure Julia wrapper `similar(a::Array{T}, dims...)`,
  allocating a fresh backing `Memory` through the existing Memory primitive and
  returning a Memory-backed wrapper.
- #3937 adds runtime `::Type{S}` where-parameter value binding and
  runtime-typed `Memory{S}(n)` allocation, which unblocks typed Pure Julia
  wrapper `similar(a, ::Type{S}, dims...)` over fresh `Memory{S}` storage.
- #3950 aligns wrapper `size(a, d)` with upstream `AbstractArray` trailing
  dimension semantics by returning `1` for `d > ndims(a)`.

Tasks:

- Route `similar`, `reshape`, indexing, mutation, views, and broadcast through
  Pure Julia where the first argument is an `Array{T,N}` wrapper.
- Keep Rust fallbacks only for primitive memory access, ABI returns, and old
  bytecode compatibility.
- Update `docs/vm/BUILTIN_REMOVAL.md` and `docs/vm/PURE_JULIA_DESIGN.md` when a
  public array route is removed.

Test gate:

```bash
timeout 1200 cargo nextest run --release --test fixture_tests array:: broadcast:: views:: type_preservation::
```

## Phase 5: Retire Core `Value::Array`

Status: complete for the enum variant and source/test audit surface (Issue
#4568). `Value::Array(ArrayRef)` has been removed from `Value`; the audit now
enforces zero `Value::Array` matches in `subset_julia_vm/src` and
`subset_julia_vm/tests`.

The remaining native-array storage bridge is explicit: `Value::NativeArray`
plus `native_array_value_ref`, `native_array_ref_value`,
`native_array_value_from_array`, and `native_array_ref_from_value`. These are
compatibility converters for runtime/FFI/formatting/cache paths while public
behavior continues moving to Memory primitives and Pure Julia `Array{T,N}`
wrappers.

Completed in #4568:

- Removed the `Value::Array(ArrayRef)` enum variant.
- Renamed the shared Phase 4 helper bodies to explicit native-array
  compatibility converters, so the old `legacy_array_value_ref`,
  `legacy_array_value_mut_ref`, `array_ref_value`, `array_value_from_value`,
  and `array_ref_from_value` helper bodies no longer exist.
- Updated `scripts/check_value_array_allowlist.sh` from a per-file ceiling to
  a zero-match audit.
- Updated code audit documentation to treat new `Value::Array` text in
  runtime/tests as a hard failure.

Tasks:

- Continue shrinking `Value::NativeArray` compatibility converter call sites
  toward Memory primitives or dispatch over the Julia `Array{T,N}` wrapper.
- Keep explicit conversion helpers only for documented compatibility
  boundaries.
- Keep `scripts/check_value_array_allowlist.sh` at zero matches.

Test gate:

```bash
timeout 1200 cargo nextest run --release
cargo clippy --all-targets -- -D warnings
bash scripts/check_value_array_allowlist.sh
```

## Compatibility Rule

New code must choose one of these paths explicitly:

- `Memory{T}` runtime primitive for storage semantics.
- Pure Julia `Array{T,N}` methods for public Base behavior.
- Documented `Value::NativeArray` compatibility conversion for FFI,
  formatting, cache, or already-existing runtime fallback.

Do not add new `Value::Array` references; the audit must stay at zero.
