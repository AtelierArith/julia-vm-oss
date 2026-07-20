# Crate Split Design (Issue #8640, design track #8654)

*Last updated: 2026-07-15. Original measurements taken on `main` @ 767eae0b2.*

This document is the design record for splitting the ~397k-line
`subset_julia_vm` crate into layered compilation units. It defines the target
crate set, the boundary rules (with the measured list of types/functions that
cross each boundary), the migration phasing executed by Issues #8655 (lower
layers) and #8656 (compile/vm), and the agreed completion criteria for
Issue #8449 (Compiler/VM separation).

**Scope**: design only — no code moves in this document's PR. #8655/#8656 are
the execution issues; `subset_julia_vm_aot` extraction is governed separately
by `ADR_BACKEND_STRATEGY.md` (Issue #8653).

**Completion update (Issue #9090, 2026-07-15):** the final physical split is
implemented. `subset_julia_vm_compile`, `subset_julia_vm_vm`, and
`subset_julia_vm_lowering` are independently checkable crates; the integration
crate composes their loader, macro-expansion, cache, and cancellation seams.
The coupling audit reports `compile_to_vm = 0`, `vm_to_compile = 0`, and
`vm_to_compile_tests = 0`. On the same Linux host and protocol as §6, cold
`cargo check -p subset_julia_vm --features repl` improved from the pre-split
47.9 s median to **34.41 s** (34.75 / 34.35 / 34.41 s). Warm edit medians are
**2.32 s** for VM-only (was 4.87 s), **3.19 s** for compile-only (was 4.85 s),
and **2.99 s** for Pure Julia (was 3.62 s). Touching VM leaves `_compile`
`Fresh` (0.15 s), and touching compile leaves `_vm` `Fresh` (0.14 s).

---

## 1. Goal and non-goals

**Goal**: a change to `vm/` must not recompile `compile/`, and vice versa;
a change to Pure Julia (`src/julia/*.jl`) must recompile only the thin
integration crate. This is the compile-time counterpart of #8449's
architectural goal (multiple VM backends over a shared IR / type system).

**Non-goals**:

- Not the VM-agnostic IR itself (#8440/#8449 item 6, carried by the SSA IR
  slices). The split must *not* wait for the SSA pipeline — see §4.1.
- Not the AoT crate move (#8653) — but the phasing must compose with it.
- No behavior change at any step; each phase is a pure `mod`-move +
  `Cargo.toml` refactor gated by the full test suite.

## 2. Current state (measured)

### 2.1 Size per top-level module (`subset_julia_vm/src/`)

| Module | LOC | | Module | LOC |
|---|---:|---|---|---:|
| `compile/` | 142,142 | | `repl/` | 6,241 |
| `vm/` | 107,632 | | `types/` | 5,297 |
| `aot/` | 50,223 | | `julia/` (Rust glue) | 3,082 |
| `lowering/` | 44,270 | | `ir/` | 1,163 |
| `inference_core/` | 18,843 | | root `*.rs` (unicode, register_vm, rng, builtins, loader, pipeline, promotion, …) | 13,486 |

`aot/` is `cfg(feature = "aot")`-gated (not in the default build) and leaves
for `subset_julia_vm_aot` under #8653; the always-compiled surface is ~347k
LOC in **one compilation unit**. The parser (~10k) is already split out as
`subset_julia_vm_parser` with proven effect (#8640).

### 2.2 Module-level dependency graph

Counted as `crate::<module>::` references per source module (comment lines
excluded), same methodology family as `scripts/audit_compile_vm_coupling.sh`:

| From \ To | ir | types | inference_core | lowering | compile | vm |
|---|---:|---:|---:|---:|---:|---:|
| `ir` | — | 1 | 0 | 0 | 0 | 0 |
| `types` | 0 | — | **26** | 0 | 0 | 0 |
| `inference_core` | 0 | **31** | — | 0 | 0 | 4 (aot-cfg) |
| `lowering` | 218 | 33 | 0 | — | **2** | **5** |
| `compile` | 250 | 150 | 115 | **6** | — | **131** |
| `vm` | 22 | 781 | 99 | **5** | **10** | — |

Bold = edges that violate the target layering and are addressed in §4.
Historical coupling-audit baseline at design time: `compile_to_vm` limit 128
(then **131** — main was red, Issue #8803), `vm_to_compile` limit 10 (at 10; 5
of these were `#[cfg(test)]`-only). Current main has since tightened the runtime
ratchets to `compile_to_vm = 0`, runtime `vm_to_compile = 0`, and
source-test `vm_to_compile_tests = 0` (Issue #9021).

### 2.3 `runtime_types` facade coverage

`src/runtime_types.rs` (Issue #8449) re-exports
`LatticeType`/`ConcreteType`/`ConstValue`, `MethodTable`/`MethodSig`,
`StructInfo`/`ParametricStructDef`
(owned by `subset_julia_vm_types::runtime_types` or
`subset_julia_vm_bytecode`), `TypeEnv`/
`ExceptionType`/`Effects`/`EffectBit`
(owned by `subset_julia_vm_types::runtime_types`),
`JuliaType`/`StructHierarchy` (owned by `types`), and
`ValueType`/`StructDefInfo`/`AbstractTypeDefInfo`/`PrimitiveTypeDefInfo`/
`RuntimeCompileContext` (owned by `subset_julia_vm_bytecode`), plus the
`ReflectionInferenceSession` trait (#8556). The facade preserves historical
paths while the lower crates own the shared type/bytecode data; physical
`_vm` extraction still needs the facade's compile-backed inference pieces
inverted into lower crates or integration-installed traits.

## 3. Target crate set

```
subset_julia_vm_parser        (exists, ~10k)   CST parser / lexer
        ▲
subset_julia_vm_ir            span, error, ir/, expr_heads,
                              lowering/ core (CST→Core IR, type_alias, expr helpers)
        ▲
subset_julia_vm_types         types/ (JuliaType) + inference_core/ (CoreType,
                              subtype, dispatch_resolver) + runtime_types facade
                              + promotion + runtime_constants
        ▲
subset_julia_vm_bytecode      program representation: vm/instr (Instr + operand
                              structs), vm/value (Value model, ValueType,
                              ArrayElementType), slotization, CompiledProgram,
                              builtins.rs (BuiltinOp), intrinsics.rs
        ▲                ▲
subset_julia_vm_compile   subset_julia_vm_vm
  compile/ (~142k)          vm/ interpreter (~remaining 90k+), register_vm.rs,
                            rng.rs, unicode.rs
        ▲                ▲
subset_julia_vm           integration crate (thin): pipeline.rs, api.rs, repl/,
                          loader/base_loader/stdlib_loader, julia/ (.jl embed),
                          lowering macro_runtime + closure_box (VM-backed macro
                          expansion), plotting, bins (sjulia), ffi_support
        ▲
subset_julia_vm_ffi / _web / _aot (#8653) / _runtime   (exist or planned)
```

Notes and deviations from the #8640 sketch, forced by measurement:

1. **`types` and `inference_core` must be co-located** in
   `subset_julia_vm_types`. The sketch put `types/` in the ir-crate, but the
   edge is bidirectional (26 vs. 31 references): `JuliaType::comparison`
   delegates to `CoreSubtypeEngine`, while `inference_core/type_core/convert`
   builds `CoreType` from `JuliaType`. Splitting them across crates is a
   cycle; merging them into one crate is cheap and matches their shared
   subject (the type model).
2. **A `subset_julia_vm_bytecode` crate is the load-bearing decision.** The
   crate now owns the complete shared program representation: stable wire
   identifiers (`BuiltinId`, `Intrinsic`, and their serde wire tables; Issue
   #8656), type tags (`ArrayElementType`, `ValueType`), bytecode operand
   payloads, `MakeGenerator`'s static callable spec, `PushArrayValue`'s literal
   array payload, the serialized `Instr` enum, the full `Value` model,
   `VmError`, `rng`, `StructInfo`/`ParametricStructDef`, program containers
   (`CompiledProgram`, `FunctionInfo`, `KwParamInfo`,
   `RuntimeCompileContext`), bytecode program metadata
   (`StructDefInfo`, `AbstractTypeDefInfo`, `PrimitiveTypeDefInfo`,
   `ShowMethodEntry`, `SpecializableFunction`), serialized slot metadata
   (`VarTypeTag`), and stack-bytecode finalization helpers (peephole
   optimization and slotization). Moving those definitions into a shared crate
   below both `compile` and `vm` removes the edge without waiting for the
   VM-agnostic IR (#8440). If crate
   count becomes a concern, folding `_bytecode` into `_types` is an
   acceptable fallback (one fewer crate; muddier naming and a bigger
   rebuild-on-types-change blast radius).
3. **`lowering` splits.** `compile` legitimately calls lowering-core helpers
   (6 refs: `make_broadcasted_call`, `type_alias::expand`,
   `lower_expr_from_text`, `try_stmt_into_value_expr`), so lowering-core must
   sit *below* compile → it goes into `subset_julia_vm_ir`. But
   `lowering/macro_runtime.rs` executes macro bodies on a real `Vm` and calls
   `compile_with_cache`, and `closure_box.rs` calls
   `compile::analyze_free_variables` — those two files stay in the
   integration crate, injected through a seam (§4.4). The seam is narrow:
   `macro_runtime` is referenced from `lowering/mod.rs` only, at 8 call
   sites, and nothing outside `lowering` names `macro_runtime::` directly.
4. **Leaf root modules** are assigned by their consumers: `rng.rs` (75 of its
   users are `vm` files) and `unicode.rs` go to `_vm`; `builtins.rs`
   (`BuiltinOp`, serde-serialized into bytecode) and `intrinsics.rs` go to
   `_bytecode`; `promotion.rs` and `runtime_constants.rs` go to `_types`.
5. **`register_vm.rs`** goes to `_vm` (it depends only on `vm` + `rng`
   today). Both engines lower from the same shared representation, per the
   REGISTER_VM.md / ADR_BACKEND_STRATEGY.md coexistence rules.

## 4. Boundary rules

General rule (extends the current ratchet policy, CODE_AUDITS.md): **all
dependencies point downward** in the §3 diagram; anything that must flow
upward crosses through a trait defined in the lower crate and implemented /
installed by the integration crate.

### 4.1 `compile` → `vm` (131 refs) — resolved by `_bytecode` + backend finalizer

Measured symbol traffic from `compile/` into `vm::*`:

| Category | Symbols (ref counts from `use`-lines + paths) | New owner |
|---|---|---|
| Bytecode ISA | `Instr` (65), VM-owned operands `MakeGeneratorOperands` / `GeneratorCallable`, `SpecializableFunction`; bytecode-only operands (`CallVarKwargsSplat`, `TypedDispatchStoreDict`, `DynamicCallCandidate`, `ModuleOperands`, `RegisterEnumOperands`, `StaticParametricCall`, `StaticParamBinding`, slot-call operands, etc.) already moved to `_bytecode`; `Instr::VARIANTS` (cache fingerprint, #8626) | `_bytecode` |
| Value model | `Value`/`SymbolValue` (4), helpers `is_rational_type_name`, `is_array_wrapper_struct_name`, `RegexValue`, `ModuleValue::new`; `ArrayElementType`, `ValueType`, `array_element_type_to_julia_type`, `julia_array_type_for_ndims`, and `parse_parametric_params` | `_bytecode` |
| Program container | `CompiledProgram` (6), `RuntimeCompileContext`, `StructDefInfo` (4), `AbstractTypeDefInfo` (2) | `_bytecode` |
| Stack-VM finalization | `peephole::optimize`, `SlotInfo`, `build_slot_info`, and `slotize_code` have moved to `_bytecode`; runtime signature projection (`expanded_param_types_for_call`, `derived_runtime_signature`) is bytecode-owned with the `FunctionInfo` program metadata | `_bytecode` |

Rule: after the split, `compile` emits `Vec<Instr>` in terms of `_bytecode`
types and bytecode-level finalization helpers. The remaining stack-VM backend
composition concern (#8449 item 3) is where the physical `_vm`/integration
crate boundary wires finalization around the VM-owned program container;
`compile` then has **zero** references upward. This deliberately does *not*
depend on the SSA IR: when
SSA lowering (#8552, currently `compile/ssa_ir/` emitting `Instr` behind
`SJULIA_SSA_PIPELINE`) matures, it slots into the same shape — SSA is an
internal detail of `_compile`, its output is still `_bytecode::Instr` until a
register-VM backend adds a second lowering target.

### 4.2 `vm` → `compile` (10 refs) — resolved by the `runtime_types` facade

Runtime (non-test) dependencies remaining:

| Site | Compiler symbol | Resolution |
|---|---|---|
| `vm/builtins_reflection/mod.rs:14` | `runtime_types::bridge::{…}` (facade exports for inference-session payload conversions) | resolved: bridge conversions now live under `runtime_types::bridge`; `compile::bridge` is only a compiler-side compatibility re-export |
| `vm/builtins_reflection/mod.rs` (`:17`, `:2036`, `:2276`) | `compile::effects::{EffectBit, Effects}`, `propagation::infer_function_effects` | resolved: `Effects`/`EffectBit`, expression effect inference, and the single-function body walker now live under `subset_julia_vm_types::runtime_types`; `compile::effects` is a compiler-side compatibility/export layer plus whole-program propagation |
| `vm/exec/call_function_variable.rs:2386` | `compile::infer_parametric_type_args` | resolved: owned by `runtime_types::parametric` and re-exported through the facade |
| 5 `#[cfg(test)]` sites | `compile_with_cache`, `compile_core_program` | dev-dependency direction is fine once tests live in the integration crate (see §5, Phase 2) |

Ownership moves per #8557/#9090: `LatticeType`, `ConstValue`, and bridge
conversions have moved from `compile/` into the lower runtime/type facades;
`MethodTable`, `MethodSig`, `StructInfo`, and `ParametricStructDef` are
bytecode-owned;
`infer_function_effects`/expression effect inference plus
`TypeEnv`/`ExceptionType`/`Effects`/`EffectBit` have moved one layer lower into
`subset_julia_vm_types::runtime_types`.
The §2.3 `runtime_types` re-export list now contains no `compile`-owned names.

### 4.3 `types` ↔ `inference_core` — co-located, internal edge allowed

Both directions stay legal *inside* `subset_julia_vm_types`. The
`From<&aot::types::StaticType> for CoreType` impl (the only
`inference_core → aot` edge, `cfg(feature = "aot")`) moves to the AoT crate
per ADR_BACKEND_STRATEGY.md consequence 1 (local-type rule).

### 4.4 `lowering` → `vm`/`compile` — macro-expansion seam

`lowering`-core (in `_ir`) defines an object-safe trait, e.g.:

```rust
pub trait MacroExpander {
    fn expand(&mut self, call: &MacroCallSite) -> Result<LoweredExpansion, LowerError>;
}
```

The 8 call sites in `lowering/mod.rs` route through an
`Option<&mut dyn MacroExpander>` (or an installed callback), and
`macro_runtime.rs` + `closure_box.rs` move to the integration crate as the
VM-backed implementation. Programs without macros lower with `None`; the
pipeline always installs the real expander. `vm/builtins_macro/`'s own use of
`Lowering` (runtime `Meta.parse` / `eval` / `ir_conversion`) is a *downward*
edge after the split (`_vm` → `_ir`) and needs no seam.

### 4.5 Audit evolution

`scripts/audit_compile_vm_coupling.sh` remains the ratchet during Phases 0–1
(fix #8803 first — main is red at `compile_to_vm = 131 > 128`). Phase 2 adds
per-boundary checks for the new edges (e.g. `vm → lowering-macro`,
`bytecode → compile/vm` must stay zero); once the crates exist, **the
compiler enforces the boundaries** and the script degrades into a guard
against re-merging (workspace `Cargo.toml` dependency review).

## 5. Migration phasing

**Phase 0 — prerequisites (tracked: #8803, #8556, #8557, #8653)**

- Fix the red ratchet on main (#8803).
- #8556 (reflection via facade) and #8557 (type-ownership move) drive
  `vm_to_compile` runtime refs to 0 — these are #8449 work items, not crate
  work. As of Issue #9021, the runtime ratchet is 0; the remaining 4 refs are
  test-only helpers to move with the integration crate in Phase 2.
- #8653 extracts `subset_julia_vm_aot`, removing the `aot` feature plumbing
  from the crate being split.

**Phase 1 — lower layers (#8655)**

1. `subset_julia_vm_ir`: move `span.rs`, `error/`, `ir/`, `expr_heads.rs`
   (mechanical; only depends on `_parser`).
2. Macro seam (§4.4), then move lowering-core into `_ir`;
   `macro_runtime`/`closure_box` stay behind the trait in the main crate.
3. `subset_julia_vm_types`: move `types/` + `inference_core/` +
   `runtime_types.rs` + `promotion.rs` + `runtime_constants.rs` together
   (§4.3 cycle forbids splitting them).
4. After each step: full nextest, iOS-sim FFI build, and the §6 measurement
   protocol; record numbers in `benchmarks/results/` style as #8655 requires.

Risk is low: all edges out of these modules already point downward
(§2.2 — `ir` and `types`+`inference_core` have no compile/vm dependencies
outside the aot-cfg impl handled in Phase 0).

**Phase 2 — compile/vm (#8656; requires Phase 1 + #8449 ratchet at 0)**

1. Finish the remaining pre-extraction seams discovered during Phase 2:
   the `runtime_types` facade pieces have moved below `compile/`; bridge
   conversions, method-table data, and `infer_parametric_type_args` are already
   owned by lower runtime/type facades; `infer_function_effects`/expression
   effect inference plus `TypeEnv`/`ExceptionType`/`Effects`/`EffectBit` are
   owned by `subset_julia_vm_types::runtime_types`.
   Inference-session construction now crosses an
   installed `ReflectionInferenceFactory`, and loader macro-registry calls cross
   `lowering::macros_registry::MacroRegistry` instead of naming `base_loader` /
   `stdlib_loader` directly. The residual pure VM helpers
   (`parse_parametric_params`, `typed_scalar_binary_instr`, runtime signature
   projection) are bytecode-owned. The first visibility-sweep slice moved
   inference cache-key policy (`InferenceCacheKey`, `CacheArgType`, and the
   const-specialization decision) into `_types`, so AoT specialization no
   longer imports those keys through `compile::abstract_interp::engine`. A
   follow-on REPL visibility slice routes `repl::{converters,globals}` value
   model imports (`Value`, `ArrayValue`, `StructInstance`, `RngInstance`, native
   array helpers, regex/memory helpers) directly through
   `subset_julia_vm_bytecode` instead of the `crate::vm` compatibility exports.
   A subsequent `repl::session` slice does the same for bytecode-owned session
   value metadata (`MemoryValue`, `FunctionValue`, `StructInstance`, `ValueType`,
   native array helpers), leaving only VM execution/statistics/transplant/linalg
   materialization boundaries on `crate::vm`. The host API slice then routes
   `api`/`ffi_support` value metadata (`Value`, `StructInstance`, `RustBigFloat`,
   native array predicate) through `subset_julia_vm_bytecode`, leaving VM
   execution/formatting/linalg materialization boundaries on `crate::vm`. The
   `macro_runtime` slice routes macro expansion AST/value bridge metadata
   (`Value`, `ExprValue`, `SymbolValue`, `GlobalRefValue`, `TupleValue`,
   `StructInstance`, `ValueType`, `ArrayRef`) through `subset_julia_vm_bytecode`,
   leaving VM execution/error boundaries on `crate::vm`. The
   `runtime_constants`/`expr_heads` slice then routes shared constant `Value`
   and canonical `ExprValue` head lookup through `subset_julia_vm_bytecode`.
   The plotting slice routes plot artifact `Value`/`StructInstance`/
   `TupleValue` imports through `subset_julia_vm_bytecode`, leaving linalg
   materialization helpers on `crate::vm`. The method-table visibility
   slice routes dispatch return `ValueType` tags through
   `subset_julia_vm_bytecode`. The `register_vm.rs` slice routes shared
   program/value/error metadata (`CompiledProgram`, `FunctionInfo`, `Instr`,
   `Intrinsic`, `Value`, `ValueType`, `VarTypeTag`, `VmError`) through
   `subset_julia_vm_bytecode`, leaving VM execution helpers on `crate::vm`.
   The `repl/tests.rs` slice routes test-only result assertion `Value` imports
   through `subset_julia_vm_bytecode`. The FFI/Web boundary slice adds direct
   `subset_julia_vm_bytecode` dependencies for C ABI value formatting/result
   conversion and wasm benchmark `CompiledProgram` storage, leaving VM
   execution and host formatting helper APIs on `subset_julia_vm`. The CLI/bin
   slice routes `sjulia` and measurement-bin program/value metadata
   (`CompiledProgram`, `FunctionInfo`, `Instr`, `Value`, `VarTypeTag`,
   `StructInstance`, native array helpers) through `subset_julia_vm_bytecode`,
   leaving only VM execution/metrics gates on `subset_julia_vm::vm`. The
   benchmark slice routes bench-only `CompiledProgram`/`Value` imports through
   `subset_julia_vm_bytecode`, leaving benchmark VM construction and feature
   toggles on `subset_julia_vm::vm`. The test/example slice routes
   integration-test and example references to bytecode-owned program/value
   metadata through `subset_julia_vm_bytecode`, leaving VM construction,
   profiler, metrics, and session cache toggles on `subset_julia_vm::vm`.
   The doc-example sweep removes the last `ValueType` reference routed through
   the VM facade. The residual helper-facade sweep removes the compatibility
   exports for `vm::util::parse_parametric_params`, `vm::typed_scalar_binary_instr`,
   and test-only runtime signature projection; VM/compiler callers now name
   `subset_julia_vm_bytecode` directly for those helpers. The REPL compile
   facade slice routes persistent/delta compile entry points, hard-scope and
   opaque-eval gates, and module-body binding collection through
   `compile::repl_support`, so `repl::session` no longer names `compile::cache`,
   `pipeline_ctx`, or `collect` internals directly. The REPL VM facade slice
   routes `repl::{session,converters}` VM execution/materialization helpers
   (`Vm`, `VmMemoryStats`, reachable struct-heap compaction, and linalg array
   wrapper materialization) through `vm::repl_support`, removing direct REPL
   references to `vm::state`/`vm::builtins_linalg` internals. The host compile
   facade slices route CLI/FFI/Web/Rust API/bench/example cache
   warmup/status/test hooks and source/IR compile entry calls through
   `compile::host_support`, leaving
   direct `compile::cache` warmup/status references only inside compiler-owned
   preload-cache code and direct compile entry calls in tests that inspect
   compiler or bytecode behavior.
   The type-interning slice moves `ConcreteTypeId`, `ConcreteTypeKey`, and
   `TypeInternTable` into `subset_julia_vm_bytecode::type_intern`; VM call-site
   id/cache code imports them directly from `_bytecode`.
2. Perform the visibility sweep for `aot/`, `repl/`, fixtures, and host APIs
   that currently consume `pub(crate)` compile/vm internals.
3. Extract `subset_julia_vm_compile` (`compile/`), then `subset_julia_vm_vm`
   (`vm/` remainder + `register_vm.rs` + `rng.rs` + `unicode.rs`).
4. `subset_julia_vm` shrinks to the integration crate; integration tests and
   fixtures stay there (fixture tests exercise the whole pipeline, so their
   compile cost stops depending on `_compile`/`_vm` internals).
5. Update CLAUDE.md/AGENTS.md crate table, CODE_AUDITS clippy scope, and the
   §6 measurements on the parent issue #8640.

Every step is *movement only* — no signature or behavior changes in the same
PR, so `git log --follow` stays useful and review is diff-shaped.

## 6. Build-time expectations and measurement protocol

Reference point (ADR_BACKEND_STRATEGY.md evidence, 2026-07-02): crate-only
rebuild `cargo check -p subset_julia_vm` = **9.0 s**; the crate is one
codegen/check unit of ~347k always-compiled LOC.

Expected steady-state wins (LOC-proportional estimates, to be validated):

| Change scenario | Today rechecks | After split | Expectation |
|---|---|---|---|
| `vm/exec/*` edit | ~347k (whole crate) | `_vm` (~113k) + thin integration | check/build time roughly **⅓** of today for the check unit |
| `compile/*` edit | ~347k | `_compile` (~142k) + integration | ~**45%** |
| `src/julia/*.jl` edit | ~347k (include_str! lives in the crate) | integration crate only (~25k) | large — near-parser-crate-level rebuilds |
| `types`/`inference_core` edit | ~347k | everything above `_types` | no win (expected; rare hot path) |

Secondary effects: crates check/build in parallel; incremental caches are
per-crate so a `vm` edit cannot invalidate `compile` artifacts; clippy and
`cargo nextest --lib` scope down with `-p`.

**Protocol** (required by #8655/#8656 acceptance): for each phase, measure
before/after on the same machine — `touch <one file in scenario>` then time
`cargo check -p subset_julia_vm --features repl` and
`cargo build --profile dev-fast -p subset_julia_vm --bin sjulia --features
repl`, 3 runs, median; plus one cold full-workspace build. Record on #8640.

Honest caveat: the split adds constant overhead (more crate metadata, more
link steps for the final binary) and `_types`-layer edits rebuild everything
above them. The bet — supported by the parser-crate precedent — is that the
dominant daily edits are in `vm/`, `compile/`, and `julia/`, which all win.

## 7. Issue #8449 completion criteria (agreed definition)

The following is the normative addition to #8449's acceptance criteria
(requested from #8654; #8449's motivation is Register VM/SSA, this pins the
crate-split-readiness meaning of "separation"):

> **Crate-split-ready dependency structure (zero cycles).** #8449 is complete
> when the `compile/` ↔ `vm/` dependency structure permits the #8656 crate
> split as a *mechanical* move (mod relocation + `Cargo.toml` edits, no logic
> changes). Concretely:
>
> 1. `scripts/audit_compile_vm_coupling.sh` runtime baselines reach
>    `vm_to_compile = 0` and `compile_to_vm = 0`, where references to symbols
>    that have been re-homed below both layers (program representation →
>    `_bytecode` per CRATE_SPLIT.md §4.1; shared type metadata →
>    `runtime_types`/`_types` per §4.2) no longer count because those symbols
>    no longer live under `vm/` or `compile/`.
> 2. Stack-VM finalization primitives (peephole, slotization) live below both
>    `compile/` and `vm/` in `_bytecode`; the physical split wires them from
>    the VM/pipeline side rather than reintroducing compile→VM imports (§4.1).
> 3. Reflection builtins reach inference only through the
>    `runtime_types::ReflectionInferenceSession` seam with the engine
>    installed by the integration layer (#8556), and the §2.3 facade owns (not
>    re-exports) the shared type metadata (#8557).
>
> Verification: the ratchet at 0/0 plus a dry-run branch demonstrating the
> #8656 move compiles (it need not merge as part of #8449).

Conversely, #8449 does **not** gate on the VM-agnostic IR being the *only*
compiler output (acceptance item 6): a `_bytecode`-shaped shared ISA
satisfies the separation goal; register-VM lowering targets are additive
(REGISTER_VM.md).

## 8. Risks and open questions

1. **`runtime_types` facade physical inversion.** The source ratchet counts
   facade crossings as zero, but a real `_vm` crate cannot depend on compile
   bridge/effects/method-table code. Move those data models into `_types` /
   `_bytecode`; inference-engine construction already sits behind an
   integration-installed factory.
2. **Visibility churn.** The mechanical split will turn many `pub(crate)`
   compile/vm APIs into cross-crate APIs consumed by AoT, REPL, fixtures, FFI,
   and web surfaces. Keep those changes mechanical and do not mix behavior
   changes into the extraction PRs. Prefer deleting compatibility-export usage
   first when the owned lower-crate API already exists.
3. **bincode caches (#8611/#8626)**: `Instr`/`BuiltinOp` serialize by variant
   order; moving their *definitions* between crates is layout-neutral, but
   any reordering during the move corrupts caches. The #8626 header
   fingerprint (`hash_enum(…, Instr::VARIANTS)`) must keep hashing the same
   list from its new crate. Checklist: CHECKLISTS.md "クレート分割・モジュール
   移動時の影響チェック".
4. **Cache-embedding relink**: building with `SJULIA_BASE_CACHE` set forces
   relink of all dependents — after the split "all dependents" is more
   crates; keep the two-step cache build procedure unchanged and re-measure.
5. **Workspace plumbing**: new crates need `default-members` entries,
   `[lints]` inheritance, feature forwarding (`repl` stays integration-only;
   `aot`/`cranelift` leave with #8653), and `subset_julia_vm_ffi`'s
   `[lib] name = "subset_julia_vm"` collision rule must be re-checked (new
   crates use distinct lib names; only `_ffi` aliases).
6. **Parallel-agent merge races**: the split reshuffles thousands of `use`
   paths; long-lived split branches will conflict with daily work. Mitigate
   by landing each Phase step within a day and re-exporting old paths
   (`pub use`) from `subset_julia_vm` during a deprecation window so
   in-flight branches keep compiling.

## References

- Issues: #8640 (parent), #8654 (this design), #8655/#8656 (execution),
  #8449 (separation), #8556/#8557 (facade), #8611/#8626 (caches),
  #8653/#8639 (AoT crate, ADR), #8440/#8552 (SSA IR), #8448 (register VM),
  #6922 (`_ffi` crate precedent), #8803 (red ratchet found during this
  design's measurement)
- Docs: `ARCHITECTURE_OVERVIEW.md` §6, `ADR_BACKEND_STRATEGY.md`,
  `REGISTER_VM.md`, `SSA_IR.md`, `CACHE_ARCHITECTURE.md`, `CODE_AUDITS.md`
  (coupling ratchet), `CHECKLISTS.md` (split impact checklist)
- Measurement: `scripts/audit_compile_vm_coupling.sh`; module graph via
  `crate::<mod>::` reference counting (§2.2)
