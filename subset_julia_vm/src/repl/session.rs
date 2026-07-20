// Issue #10906 (Phase 1c of #10869): zero real unwrap_used/expect_used sites
// in production code — the sole prior site (`delta_eligible implies a prior
// persistent compile`) was converted to a guarded `match` + typed
// `REPLResult::error`. Every remaining unwrap/expect call in this file lives
// inside cfg(test) modules, which carry an explicit allow (test code may use
// these freely, per docs/vm/PANIC_FREE.md).
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use std::collections::HashMap;

use crate::compile::repl_support;
use crate::ir::core::{
    AbstractTypeDef, Block, DefinitionOrderCursor, EnumDef, Expr, Function, Literal, MacroDef,
    Module, PrimitiveTypeDef, Program, Stmt, StructDef, TypeAliasDef, UsingImport,
};
use crate::loader;
use crate::lowering::Lowering;
use crate::parser::Parser;
use crate::rng::StableRng;
use crate::span::Span;
use crate::vm::{
    repl_support::{self as vm_repl, Vm},
    ReplAppendDefinitionCounts, ReplAppendDefinitionStarts,
};
use subset_julia_vm_bytecode::value::{
    enum_registry::EnumRegistryTransaction, is_native_array_value, native_array_value_ref,
    ArrayData, FunctionValue, MemoryValue, StructInstance, Value,
};
use subset_julia_vm_bytecode::{
    CompiledProgram, DefineRuntimeNominalOperands, Instr, ReplDefinitionActivation,
    RuntimeNominalActivation, RuntimeNominalDefInfo, ValueType,
};

use super::converters::{
    callable_value_to_expr, empty_array_init_expr, potential_rebindings_of,
    struct_instance_to_literal, value_literal_leaf_estimate, value_to_init_expr, value_to_literal,
    value_to_module_init_expr,
};

/// Upper bound on the number of scalar leaves a persisted global may reconstruct
/// as an AST init literal. Beyond this the global is value-carried across evals
/// (the struct heap is transplanted anyway) instead of rebuilding — and, worse,
/// re-cloning every eval — a giant `Expr` literal. A push!-based `@animate`
/// animation snapshots the cumulative path per frame, so its frames hold
/// O(frames^2) points; the stock 9000-step / `every 80` Aizawa sample is ~500k
/// leaves, which OOM-aborted the iOS REPL on the next eval (Issue #9229). The cap
/// is far above any hand-written struct/array literal a user would type.
const MAX_PERSISTED_GLOBAL_LITERAL_LEAVES: usize = 65_536;
use super::globals::{REPLGlobals, REPLResult};

/// A live VM re-entered with an LV2 relocatable delta main, ready for `run()`
/// (Issue #9199). Returned by `REPLSession::try_live_delta_run` so `eval` can
/// join the shared post-run path with the fresh-build path.
struct PreparedLiveDelta {
    vm: Vm<StableRng>,
    main_scope_names: std::collections::HashSet<String>,
    next_persistent: Option<repl_support::ReplPersistentCompile>,
    first_appended_global_slot_index: usize,
    first_appended_function_index: usize,
    appended_function_names: Vec<String>,
    source_function_indices: Vec<usize>,
    first_appended_struct_index: usize,
    appended_struct_names: Vec<String>,
    first_appended_abstract_type_index: usize,
    appended_abstract_type_names: Vec<String>,
    first_appended_primitive_type_index: usize,
    appended_primitive_type_names: Vec<String>,
    first_appended_enum_index: usize,
    appended_enum_names: Vec<String>,
    definition_activations: Vec<ReplDefinitionActivation>,
    runtime_nominal_templates: Vec<DefineRuntimeNominalOperands>,
    enum_registry_transaction: Option<EnumRegistryTransaction>,
}

/// Metadata needed to prove that an errored live VM and its compiler snapshot
/// describe the same source-ordered function-definition prefix (Issue #9784).
struct LiveErrorRecoveryPlan {
    first_appended_global_slot_index: usize,
    first_appended_function_index: usize,
    appended_function_names: Vec<String>,
    source_function_indices: Vec<usize>,
    runtime_function_qualified_names: HashMap<usize, String>,
    first_appended_struct_index: usize,
    appended_struct_names: Vec<String>,
    first_appended_abstract_type_index: usize,
    appended_abstract_type_names: Vec<String>,
    first_appended_primitive_type_index: usize,
    appended_primitive_type_names: Vec<String>,
    first_appended_enum_index: usize,
    appended_enum_names: Vec<String>,
    definition_activations: Vec<ReplDefinitionActivation>,
    runtime_nominal_templates: Vec<DefineRuntimeNominalOperands>,
}

/// Source declarations that actually committed inside modules before a
/// catchable toplevel error. A failed module cannot be replayed wholesale: its
/// body would throw again and declarations after the failure would be revived.
/// This projection is converted into inert module shells for later full
/// rebuilds (Issue #11721).
#[derive(Default)]
struct RecoveredModuleReplay {
    module_paths: std::collections::HashSet<String>,
    function_names: std::collections::HashSet<String>,
    struct_names: std::collections::HashSet<String>,
    abstract_type_names: std::collections::HashSet<String>,
    primitive_type_names: std::collections::HashSet<String>,
    enum_names: std::collections::HashSet<String>,
    runtime_nominals: Vec<RuntimeNominalActivation>,
}

impl RecoveredModuleReplay {
    fn from_reached(
        plan: &LiveErrorRecoveryPlan,
        reached: &crate::vm::ReachedReplDefinitionPrefix,
        current_input_module_paths: &std::collections::HashSet<String>,
        reached_module_paths: &std::collections::HashSet<String>,
        current_input_module_function_positions: &HashMap<String, usize>,
        current_input_module_binding_positions: &HashMap<String, usize>,
        module_globals: &HashMap<String, Value>,
    ) -> Self {
        let mut module_paths = reached_module_paths
            .intersection(current_input_module_paths)
            .cloned()
            .collect::<std::collections::HashSet<_>>();
        let mut function_names: std::collections::HashSet<String> = plan
            .source_function_indices
            .iter()
            .take(reached.function_count)
            .filter_map(|index| {
                index
                    .checked_sub(plan.first_appended_function_index)
                    .and_then(|offset| plan.appended_function_names.get(offset))
            })
            .cloned()
            .collect();
        let runtime_nominals = reached.runtime_nominal_activations.clone();
        let reached_source_frontier = runtime_nominals
            .iter()
            .map(|activation| activation.span.start)
            .max();
        function_names.extend(
            reached
                .runtime_function_indices
                .iter()
                .filter_map(|index| plan.runtime_function_qualified_names.get(index))
                .filter(|name| {
                    current_input_module_function_positions
                        .get(*name)
                        .is_some_and(|position| {
                            reached_source_frontier.is_none_or(|frontier| *position <= frontier)
                        })
                })
                .cloned(),
        );
        let struct_names: std::collections::HashSet<String> = plan.appended_struct_names
            [..reached.struct_count]
            .iter()
            .cloned()
            .collect();
        let abstract_type_names: std::collections::HashSet<String> = plan
            .appended_abstract_type_names[..reached.abstract_type_count]
            .iter()
            .cloned()
            .collect();
        let primitive_type_names: std::collections::HashSet<String> = plan
            .appended_primitive_type_names[..reached.primitive_type_count]
            .iter()
            .cloned()
            .collect();
        let enum_names: std::collections::HashSet<String> = plan.appended_enum_names
            [..reached.enum_count]
            .iter()
            .cloned()
            .collect();

        // Module values are definition-owned and may not occupy a frame-0 slot.
        // A reached qualified declaration proves that every containing module
        // on its owner path began execution.
        let mut reached_qualified_names = function_names
            .iter()
            .chain(&struct_names)
            .chain(&abstract_type_names)
            .chain(&primitive_type_names)
            .chain(&enum_names)
            .cloned()
            .collect::<Vec<_>>();
        reached_qualified_names.extend(
            runtime_nominals
                .iter()
                .filter(|activation| {
                    reached_source_frontier.is_none_or(|frontier| activation.span.start <= frontier)
                })
                .map(|activation| match &activation.definition {
                    RuntimeNominalDefInfo::Struct(definition) => definition.source.name.clone(),
                    RuntimeNominalDefInfo::AbstractType(definition) => definition.name.clone(),
                    RuntimeNominalDefInfo::PrimitiveType(definition) => definition.name.clone(),
                    RuntimeNominalDefInfo::Enum(definition) => definition.name.clone(),
                }),
        );
        reached_qualified_names.extend(
            module_globals
                .keys()
                .filter(|name| {
                    current_input_module_binding_positions
                        .get(*name)
                        .is_some_and(|position| {
                            reached_source_frontier.is_none_or(|frontier| *position <= frontier)
                        })
                })
                .cloned(),
        );
        for qualified in reached_qualified_names {
            let mut owner = qualified.rsplit_once('.').map(|(owner, _)| owner);
            while let Some(path) = owner {
                if current_input_module_paths.contains(path) {
                    module_paths.insert(path.to_string());
                }
                owner = path.rsplit_once('.').map(|(parent, _)| parent);
            }
        }
        Self {
            module_paths,
            function_names,
            struct_names,
            abstract_type_names,
            primitive_type_names,
            enum_names,
            runtime_nominals,
        }
    }
}

fn full_compile_publication_recovery_plan(
    compiled: &CompiledProgram,
    current_input_recoverable_function_count: usize,
    current_input_using_count: usize,
    current_input_module_count: usize,
) -> Option<LiveErrorRecoveryPlan> {
    let code = compiled.code.get(compiled.entry..).unwrap_or_default();
    let runtime_nominal_templates = code
        .iter()
        .filter_map(|instruction| match instruction {
            Instr::DefineRuntimeNominal(operands) => Some((**operands).clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    if current_input_recoverable_function_count == 0
        && current_input_using_count == 0
        && current_input_module_count == 0
        && runtime_nominal_templates.is_empty()
    {
        return None;
    }
    let mut definition_activations = Vec::new();
    let mut function_ids = Vec::new();
    let mut struct_ids = Vec::new();
    let mut abstract_ids = Vec::new();
    let mut primitive_ids = Vec::new();
    let mut enum_ids = Vec::new();
    for instruction in code {
        let activation = match instruction {
            Instr::DefineEvalFunction(index) => {
                function_ids.push(*index);
                Some(ReplDefinitionActivation::Function(*index))
            }
            Instr::DefineEvalStruct(type_id) => {
                struct_ids.push(*type_id);
                Some(ReplDefinitionActivation::Struct(*type_id))
            }
            Instr::DefineEvalAbstractType(type_id) => {
                abstract_ids.push(*type_id);
                Some(ReplDefinitionActivation::AbstractType(*type_id))
            }
            Instr::DefineEvalPrimitiveType(type_id) => {
                primitive_ids.push(*type_id);
                Some(ReplDefinitionActivation::PrimitiveType(*type_id))
            }
            Instr::RegisterEnum(operands) => {
                let enum_id = compiled.enum_defs.iter().position(|definition| {
                    definition.name == operands.type_name && definition.members == operands.members
                })?;
                enum_ids.push(enum_id);
                Some(ReplDefinitionActivation::Enum(enum_id))
            }
            _ => None,
        };
        if let Some(activation) = activation {
            if !definition_activations.contains(&activation) {
                definition_activations.push(activation);
            }
        }
    }
    fn contiguous_names<T>(
        ids: &[usize],
        definitions: &[T],
        name: impl Fn(&T) -> &str,
    ) -> Option<(usize, Vec<String>)> {
        if ids.is_empty() {
            return Some((definitions.len(), Vec::new()));
        }
        let first = ids.iter().copied().min()?;
        let last = ids.iter().copied().max()?;
        let expected_len = last.checked_sub(first)?.checked_add(1)?;
        let unique = ids
            .iter()
            .copied()
            .collect::<std::collections::HashSet<_>>();
        if unique.len() != expected_len || last >= definitions.len() {
            return None;
        }
        Some((
            first,
            definitions[first..=last]
                .iter()
                .map(|definition| name(definition).to_string())
                .collect(),
        ))
    }

    fn function_suffix_names<T>(
        ids: &[usize],
        definitions: &[T],
        name: impl Fn(&T) -> &str,
    ) -> Option<(usize, Vec<usize>, Vec<String>)> {
        if ids.is_empty() {
            return Some((definitions.len(), Vec::new(), Vec::new()));
        }
        let unique = ids
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>();
        let first = unique.iter().next().copied()?;
        if unique.iter().any(|index| *index >= definitions.len()) {
            return None;
        }
        Some((
            first,
            ids.iter().copied().fold(Vec::new(), |mut ordered, index| {
                if !ordered.contains(&index) {
                    ordered.push(index);
                }
                ordered
            }),
            definitions[first..]
                .iter()
                .map(|definition| name(definition).to_string())
                .collect(),
        ))
    }

    // A full compile can append compiler-generated specialization/helper bodies
    // after the source methods that carry `DefineEvalFunction` markers. Recovery
    // retains that whole aligned function-table suffix while counting only the
    // marker-bearing front as source methods (Issue #11683).
    let (first_appended_function_index, source_function_indices, appended_function_names) =
        function_suffix_names(&function_ids, &compiled.functions, |function| {
            function.name.as_str()
        })?;
    // Module bodies are compiled outside `compiled.entry..`, while their
    // `DefineFunction` markers execute when main enters those bodies. The VM
    // records exact final table indices; `FunctionInfo.name` at that index is
    // already owner-qualified, so no source-span/leaf-name inference is needed.
    let runtime_function_qualified_names = compiled
        .functions
        .iter()
        .enumerate()
        .map(|(index, function)| (index, function.name.clone()))
        .collect();
    let (first_appended_struct_index, appended_struct_names) =
        contiguous_names(&struct_ids, &compiled.struct_defs, |definition| {
            definition.name.as_str()
        })?;
    let (first_appended_abstract_type_index, appended_abstract_type_names) =
        contiguous_names(&abstract_ids, &compiled.abstract_types, |definition| {
            definition.name.as_str()
        })?;
    let (first_appended_primitive_type_index, appended_primitive_type_names) =
        contiguous_names(&primitive_ids, &compiled.primitive_types, |definition| {
            definition.name.as_str()
        })?;
    let (first_appended_enum_index, appended_enum_names) =
        contiguous_names(&enum_ids, &compiled.enum_defs, |definition| {
            definition.name.as_str()
        })?;

    Some(LiveErrorRecoveryPlan {
        // Full recompiles do not append to a parked frame-0 slot table, so
        // suffix rollback is deliberately disabled for this recovery shape.
        first_appended_global_slot_index: usize::MAX,
        first_appended_function_index,
        appended_function_names,
        source_function_indices,
        runtime_function_qualified_names,
        first_appended_struct_index,
        appended_struct_names,
        first_appended_abstract_type_index,
        appended_abstract_type_names,
        first_appended_primitive_type_index,
        appended_primitive_type_names,
        first_appended_enum_index,
        appended_enum_names,
        definition_activations,
        runtime_nominal_templates,
    })
}

/// Project the live VM's executed frame-0 writes onto Julia value bindings.
/// Function/nominal/type-alias publication writes are definition metadata, not
/// value rebindings; every other executed write is authoritative even when it
/// originated inside a called function rather than the current main AST
/// (Issue #9784).
fn is_main_binding_name(name: &str) -> bool {
    !name.contains('.')
}

fn binding_belongs_to_any_module(
    name: &str,
    module_paths: &std::collections::HashSet<String>,
) -> bool {
    module_paths.iter().any(|path| {
        name.strip_prefix(path)
            .is_some_and(|suffix| suffix.starts_with('.'))
    })
}

fn runtime_value_rebindings(
    written: &std::collections::HashSet<String>,
    explicit_global_writes: &std::collections::HashSet<String>,
    main_scope_names: &std::collections::HashSet<String>,
    nonvalue_bindings: &std::collections::HashSet<String>,
    alias_bindings: &std::collections::HashSet<String>,
) -> std::collections::HashSet<String> {
    written
        .iter()
        .filter(|name| {
            is_main_binding_name(name)
                && (main_scope_names.contains(*name) || explicit_global_writes.contains(*name))
        })
        .filter(|name| !repl_support::is_runtime_import_metadata_binding(name))
        .filter(|name| !nonvalue_bindings.contains(*name) && !alias_bindings.contains(*name))
        .cloned()
        .collect()
}

/// REPL session that maintains state across evaluations.
pub struct REPLSession {
    /// Thread that created this session.  Used in `debug_assert!` guards to
    /// catch accidental cross-thread use of a session handle.  `REPLSession`
    /// contains `Rc`/`RefCell` state and is `!Send` + `!Sync`; the C ABI
    /// erases those markers (raw pointer), so callers MUST ensure every
    /// `repl_session_eval` / `repl_session_reset` / `repl_session_free` call
    /// runs on the same thread as the matching `repl_session_new`.
    /// (Issue #9056, Issue #8214, Issue #8675; see `docs/vm/SINGLE_THREADED_VM.md`)
    #[cfg(debug_assertions)]
    owner_thread: std::thread::ThreadId,
    /// Random seed for RNG
    seed: u64,
    /// Persistent global variables
    globals: REPLGlobals,
    /// Previously defined functions (accumulated across evaluations). One entry
    /// per method: `f(x::Int)` and `f(x::Float64)` are two entries with the same
    /// name, so multiple dispatch is preserved across evaluations (Issue #9173).
    functions: Vec<Function>,
    /// Fast index for method signature (name + parameter types) -> index in
    /// `functions`. Keyed by signature, not bare name, so defining a new method
    /// of an existing generic function EXTENDS it instead of replacing it
    /// (Issue #9173). See `method_signature_key`.
    function_index: HashMap<String, usize>,
    /// Previously defined macros (accumulated across evaluations), keyed by name
    /// in `macro_index`. Carried into the lowering of later evaluations so a
    /// macro defined in one expression is usable by a later one (Issue #9172).
    macros: Vec<MacroDef>,
    /// Fast index for macro name -> index in `macros`.
    macro_index: HashMap<String, usize>,
    /// Previously defined structs (accumulated across evaluations)
    structs: Vec<StructDef>,
    /// Fast index for struct name -> index in `structs`
    struct_index: HashMap<String, usize>,
    /// Previously defined abstract types (accumulated across evaluations),
    /// symmetric with `structs` (Issue #9701). Without this, an
    /// `abstract type Animal end` from an earlier eval is dropped from the next
    /// eval's merged program: `Animal` stops resolving as a `Type` value
    /// (`d isa Animal` → UndefVarError) and `f(a::Animal)` methods stop
    /// matching (MethodError) — in BOTH eval models.
    abstract_types: Vec<AbstractTypeDef>,
    /// Fast index for abstract type name -> index in `abstract_types`
    abstract_type_index: HashMap<String, usize>,
    /// Previously defined primitive types (accumulated across evaluations),
    /// symmetric with `structs` (Issue #9701).
    primitive_types: Vec<PrimitiveTypeDef>,
    /// Fast index for primitive type name -> index in `primitive_types`
    primitive_type_index: HashMap<String, usize>,
    /// Previously defined top-level type aliases. These are seeded into the
    /// next lowering pass before its current-source pre-scan, so an earlier
    /// evaluation remains visible before a same-eval later redefinition
    /// (Issue #11086).
    type_aliases: Vec<TypeAliasDef>,
    /// Fast index for top-level type alias name -> index in `type_aliases`.
    type_alias_index: HashMap<String, usize>,
    /// Previously defined enums (accumulated across evaluations), symmetric
    /// with `structs` (Issue #9701). NOTE: `@enum` lowers to a `Stmt::EnumDef`
    /// inside `main` (`Program.enums` stays empty), so these are collected from
    /// the main statements and re-merged back as main statements.
    enums: Vec<EnumDef>,
    /// Fast index for enum name -> index in `enums`
    enum_index: HashMap<String, usize>,
    /// Exact member constants that completed for a partially published enum.
    /// Absence means every source member was published normally (Issue #11652).
    enum_published_members: HashMap<String, Vec<String>>,
    /// Main-owned nominal bindings first published by a reached runtime site.
    /// A later root declaration is a cross-input redefinition, while ordinary
    /// root-only programs retain the established replay behavior (Issue #11684).
    runtime_nominal_names: std::collections::HashSet<String>,
    /// Previously defined modules (accumulated across evaluations)
    modules: Vec<Module>,
    /// Fast index for module name -> index in `modules`
    module_index: HashMap<String, usize>,
    /// Imported modules via `using` (accumulated across evaluations)
    usings: Vec<UsingImport>,
    /// Names of the modules auto-imported at construction (e.g. `InteractiveUtils`).
    /// The delta path can't resolve a reference to ANY module (it carries no module
    /// metadata), so `persistent_delta_eligible` rejects a session that holds a
    /// module NOT in this set — but the stateless default auto-imports never block
    /// it (Issue #9199 LV2). Derived from the auto-`using`s, not a hard-coded name.
    auto_import_modules: std::collections::HashSet<String>,
    /// Last evaluation result (for `ans`)
    ans: Option<Value>,
    /// Evaluation counter for RNG seed variation
    eval_count: u64,
    /// Last VM's struct heap (for resolving StructRefs in display)
    last_struct_heap: Vec<StructInstance>,
    /// Global variable types (for type inference in next compilation)
    /// Maps variable name -> (struct_name, type_id) for Struct types
    /// or variable name -> ValueType for other types
    global_types: HashMap<String, ValueType>,
    /// Struct names for variables (used to resolve type_id from struct_table)
    global_struct_names: HashMap<String, String>,
    /// Persisted module-level mutable state across evaluations, keyed by the
    /// qualified constant name (e.g. `Plots._CURRENT_SERIES`). Module bodies
    /// re-run on every eval and reset their `const` initializers, so the current
    /// value is captured after each run and restored before the next one,
    /// keeping `plot!`/`scatter!` appending to the current plot (Issue #5296).
    module_globals: HashMap<String, Value>,
    /// Qualified module bindings proven by an executed `StoreGlobalAny`. Unlike
    /// source-visible module assignments, these may be created only inside a
    /// called function and therefore need direct value seeding on a fresh VM.
    /// They remain module-owned and never enter the Main `globals` mirror
    /// (Issue #9784).
    module_runtime_global_names: std::collections::HashSet<String>,
    /// Snapshot of the most recent eval's VM memory/cache counters
    /// (Issue #8625). Lets a long-running host observe struct-heap and runtime
    /// cache growth across a session; `None` before the first successful eval.
    last_vm_memory_stats: Option<vm_repl::VmMemoryStats>,
    /// Qualified paths (`M`, `M.Sub`) of every USER-DEFINED module whose
    /// `__init__` has already run in this session (Issue #9199 S4). Under the
    /// persistent model, a module is realized ONCE: the accumulate-and-recompile
    /// pass re-runs each prior module body every eval, but a module already in
    /// this set is not being (re)defined by the current input, so its `__init__`
    /// side effects must fire only on the eval that defined it — matching
    /// upstream `run_module_init` (a module's `__init__` runs once per
    /// realization; see `julia/base/loading.jl`). Populated only from the modules
    /// the user actually typed (captured BEFORE the package loader appends its
    /// modules), so package `using X` modules are never suppressed: their
    /// `__init__` establishes VM-local state that MUST re-run in each eval's fresh
    /// VM, and they stay on the fresh full-recompile path (Issue #9199 S4 /
    /// #8994 / #5296). Cleared by `reset()`.
    initialized_module_paths: std::collections::HashSet<String>,
    /// Accumulated compiled program carried across `Persistent` evals so an
    /// append-only input compiles ONLY the delta instead of re-lowering +
    /// recompiling the whole session (Issue #9199 S5). `None` before the first
    /// successful full compile. Holds the exact merged IR + reusable compile
    /// bundle that a later delta appends to; refreshed by every full-recompile
    /// eval and dropped by `reset()`. See `docs/vm/ADR_REPL_EVAL_MODEL.md` (§S5).
    persistent_compile: Option<repl_support::ReplPersistentCompile>,
    /// Wall-clock nanoseconds the most recent eval spent in its compile phase
    /// (merge + lower-fold + codegen), excluding VM construction and execution
    /// (Issue #9199 S5). `None` before the first eval. This is the quantity the
    /// epic's exit criterion tracks — per-eval compile cost — so the benchmark
    /// `benches/repl_input_delta_9199.rs` reads it to plot compile-time-vs-N and
    /// confirm the persistent path stays flat while Legacy grows O(session).
    last_compile_nanos: Option<u128>,
    /// Wall-clock nanoseconds the most recent eval spent building the fresh `Vm`
    /// from the compiled program (`Vm::new_program`), excluding the compile phase
    /// above and the subsequent `run()` (Issue #9199 live-VM slice). `None` before
    /// the first eval. Measured to CHECK whether VM construction is a SECOND
    /// O(session-length) cost the live-VM reshape must flatten — `Vm::new_program`
    /// does re-derive ~20 program-scaled tables every eval
    /// (`call_site_caches = vec![_; code.len()]`, the predecoded `ExecutableProgram`,
    /// `function_slot_maps` / `function_name_index` / `type_ancestors` /
    /// `struct_hierarchy` over ALL functions). Empirically (`benches/repl_input_delta_9199.rs`)
    /// it is NOT: vm-build is small (~single-digit ms) and roughly FLAT over the
    /// measured N, because it is dominated by the fixed Base program and the
    /// marginal per-user-definition cost is negligible next to it. So the epic's
    /// O(session) cost is concentrated in the compile phase (`last_compile_nanos`),
    /// which is what the live-VM slice must flatten; this telemetry records the
    /// negative result so a later slice can re-check it at larger N. Rust-only
    /// telemetry; not a C ABI surface.
    last_vm_build_nanos: Option<u128>,
    /// Whether this session runs under a host with a rich "graphical display"
    /// (iOS/web REPL), so `display(x)` routes a `Plot`/animation into the
    /// artifact channel instead of printing its text form (Issue #9262). Off by
    /// default so the terminal REPL keeps text-only `display`; graphical hosts
    /// opt in via `set_graphical_display(true)`.
    graphical_display: bool,
    /// The live `Vm` held across evals so an expression-only delta can run on it
    /// directly — appended + re-entered — instead of building a fresh VM whose
    /// frame-0 globals, struct heap, dispatch caches, and world would have to be
    /// re-seeded (Issue #9199 LV1; see `docs/vm/ADR_REPL_EVAL_MODEL.md`
    /// §"Live-VM slice decomposition"). Populated by every successful eval;
    /// consumed (append + re-enter) by the next LV1-eligible delta; dropped when
    /// an error cannot be projected transactionally and by `reset()`. `None`
    /// before the first successful eval.
    /// Single-threaded VM state (`!Send`/`!Sync`),
    /// held behind the opaque session handle, so no C ABI surface (Issue #9199).
    live_vm: Option<Vm<StableRng>>,
    /// Callable identity table for the program that produced persisted globals.
    /// Kept independently of `live_vm`: an unprojectable error drops the VM but
    /// deliberately retains the last successful globals, whose frozen function
    /// candidate indices still need rebasing on the next full build (#9784).
    persisted_callable_snapshot: Option<vm_repl::PersistedCallableSnapshot>,
}

impl REPLSession {
    /// Create a new REPL session with the given RNG seed.
    ///
    /// The session is initialized with `InteractiveUtils` automatically imported,
    /// matching Julia's standard REPL behavior where `versioninfo()` and other
    /// utilities are available by default.
    pub fn new(seed: u64) -> Self {
        // Automatically import InteractiveUtils, matching Julia's REPL behavior
        // Julia's REPL loads InteractiveUtils by default for convenience functions
        // like versioninfo(), supertypes(), etc.
        let default_usings = vec![UsingImport {
            module: "InteractiveUtils".to_string(),
            is_import: false,
            symbols: None, // Import all exported symbols
            is_relative: false,
            relative_level: 0,
            alias_bindings: Vec::new(),
            span: Span::new(0, 0, 0, 0, 0, 0), // Synthetic span for auto-import
        }];
        // Modules the package loader realizes from the auto-`using`s above; the
        // delta path may safely coexist with these (Issue #9199 LV2).
        let auto_import_modules = default_usings.iter().map(|u| u.module.clone()).collect();

        Self {
            #[cfg(debug_assertions)]
            owner_thread: std::thread::current().id(),
            seed,
            globals: REPLGlobals::new(),
            functions: Vec::new(),
            function_index: HashMap::new(),
            macros: Vec::new(),
            macro_index: HashMap::new(),
            structs: Vec::new(),
            struct_index: HashMap::new(),
            abstract_types: Vec::new(),
            abstract_type_index: HashMap::new(),
            primitive_types: Vec::new(),
            primitive_type_index: HashMap::new(),
            type_aliases: Vec::new(),
            type_alias_index: HashMap::new(),
            enums: Vec::new(),
            enum_index: HashMap::new(),
            enum_published_members: HashMap::new(),
            runtime_nominal_names: std::collections::HashSet::new(),
            modules: Vec::new(),
            module_index: HashMap::new(),
            usings: default_usings,
            auto_import_modules,
            ans: None,
            eval_count: 0,
            last_struct_heap: Vec::new(),
            global_types: HashMap::new(),
            global_struct_names: HashMap::new(),
            module_globals: HashMap::new(),
            module_runtime_global_names: std::collections::HashSet::new(),
            last_vm_memory_stats: None,
            initialized_module_paths: std::collections::HashSet::new(),
            persistent_compile: None,
            last_compile_nanos: None,
            last_vm_build_nanos: None,
            graphical_display: false,
            live_vm: None,
            persisted_callable_snapshot: None,
        }
    }

    fn refresh_persisted_callable_snapshot(&mut self, vm: &Vm<StableRng>, append_only: bool) {
        if append_only {
            if let Some(snapshot) = self.persisted_callable_snapshot.as_mut() {
                if vm.extend_persisted_callable_snapshot(snapshot) {
                    return;
                }
            }
        }
        self.persisted_callable_snapshot = Some(vm.persisted_callable_snapshot());
    }

    /// Enable or disable the host graphical display for this session (Issue
    /// #9262). Graphical hosts (iOS/web REPL) enable it so `display(plot(cos))`
    /// renders as an interactive figure; the terminal REPL leaves it off for
    /// text-only `display`.
    pub fn set_graphical_display(&mut self, enabled: bool) {
        self.graphical_display = enabled;
    }

    /// VM memory/cache counters captured after the most recent successful eval
    /// (Issue #8625). `None` until the first successful eval. Hosts use this to
    /// track struct-heap and runtime-cache growth over a long session.
    pub fn last_vm_memory_stats(&self) -> Option<vm_repl::VmMemoryStats> {
        self.last_vm_memory_stats
    }

    /// Wall-clock nanoseconds the most recent successful eval spent compiling
    /// (merge + global fold + codegen), excluding VM construction and execution
    /// (Issue #9199 S5). `None` before the first eval or when the eval failed
    /// before reaching the compile phase. Rust-internal telemetry used by the
    /// input-delta benchmark and available to hosts tracking REPL latency; not a
    /// C ABI surface.
    pub fn last_compile_nanos(&self) -> Option<u128> {
        self.last_compile_nanos
    }

    /// Wall-clock nanoseconds the most recent successful eval spent constructing
    /// the fresh `Vm` from the compiled program (`Vm::new_program`), excluding the
    /// compile phase ([`last_compile_nanos`](Self::last_compile_nanos)) and the
    /// `run()` that follows (Issue #9199 live-VM slice). `None` before the first
    /// eval or when the eval failed before VM construction. Complements
    /// `last_compile_nanos` so the input-delta benchmark can locate where the
    /// epic's O(session) cost lives: it turns out to be the COMPILE phase, while
    /// this VM-build phase stays small and ~flat over the measured N (Base-
    /// dominated). Recording it pins that split so the live-VM slice can target
    /// the compile phase and re-check vm-build at larger N. Rust-internal
    /// telemetry, not a C ABI surface.
    pub fn last_vm_build_nanos(&self) -> Option<u128> {
        self.last_vm_build_nanos
    }

    /// Whether this session currently holds a live `Vm` across evals (Issue #9199
    /// LV1). `true` after a successful eval (which parks its VM for a future
    /// live-append), and `false` before the first eval or after `reset()`.
    /// Rust-only observability (no C ABI surface).
    pub fn has_live_vm(&self) -> bool {
        self.live_vm.is_some()
    }

    /// Evaluate Julia code in this session.
    /// Variables defined here will persist for future evaluations.
    pub fn eval(&mut self, input: &str) -> REPLResult {
        // Guard: a session must never be used from a different thread than the
        // one that created it (Issue #9056, #8214, #8675). The raw-pointer C
        // ABI erases `!Send`/`!Sync`, so enforce the contract with a cheap
        // debug assertion. Violation indicates a host-side threading bug; the
        // session state (`Rc`/`RefCell` internals) is not safe for cross-thread
        // access even with serialization.
        #[cfg(debug_assertions)]
        debug_assert_eq!(
            std::thread::current().id(),
            self.owner_thread,
            "REPLSession::eval called from thread {:?} but was created on thread {:?}. \
             Session handles are single-threaded: marshal all calls to the owner thread \
             (see docs/vm/SINGLE_THREADED_VM.md and Issue #8675).",
            std::thread::current().id(),
            self.owner_thread,
        );
        crate::cancel::reset();

        // Parse input
        let mut parser = match Parser::new() {
            Ok(p) => p,
            Err(e) => {
                return REPLResult::error(format!("Parser init failed: {}", e), String::new())
            }
        };

        let outcome = match parser.parse(input) {
            Ok(o) => o,
            Err(e) => return REPLResult::error(format!("Parse error: {}", e), String::new()),
        };

        // Lower to IR
        // Macro expansion seam (Issue #8656): idempotent install of the VM-backed expander.
        crate::macro_runtime::install();
        let mut lowering = Lowering::new_with_usings_and_macros(input, &self.usings, &self.macros);
        let prior_alias_scope = crate::lowering::type_alias::snapshot();
        seed_prior_type_aliases(&self.type_aliases, &self.modules);
        let lambda_context = crate::lowering::LambdaContext::for_repl_fragment(self.eval_count);
        let lowered = lowering.lower_with_lambda_context(outcome, &lambda_context);
        prior_alias_scope.restore();
        let mut program = match lowered {
            Ok(p) => p,
            Err(e) => {
                return REPLResult::error(format!("{:?}: {:?}", e.kind, e.hint), String::new())
            }
        };
        // Capture aliases declared by THIS input before prior definitions are
        // re-folded. Their publication stores are definition metadata, never
        // ordinary value rebindings.
        let current_input_alias_names: std::collections::HashSet<String> = program
            .type_aliases
            .iter()
            .map(|alias| alias.name.clone())
            .collect();
        // Each REPL input is lowered independently and starts its ordinals at
        // one. Rebase the current input after every prior definition before
        // `merge_definitions` combines their separately stored function and
        // struct vectors (Issue #11028).
        let mut chronology = DefinitionOrderCursor::after_stored_definitions(
            &self.functions,
            &self.structs,
            &self.abstract_types,
            &self.primitive_types,
            &[],
            &self.modules,
            &self.macros,
            &self.enums,
        );
        chronology.append_fragment(&mut program);
        let import_only_input = is_import_only_input(&program);

        // Whether the user's input produced any top-level value statement — captured
        // BEFORE `merge_definitions`/`inject_globals` fold prior definitions and
        // globals into `main`. A definition-only eval (a bare `f(x)=…` method
        // extension, `struct`, `macro`, or `module`) has an empty user `main`.
        // Full-recompile fallback may prepend reconstructed prior-global init
        // statements to `main`, but those are not user-authored result expressions.
        // `user_main_empty` makes definition-only input return `Nothing`, matching
        // upstream rather than echoing a stale reconstructed binding (Issue #9199).
        //
        // A docstring preceding a definition lowers to a compiler-generated
        // `__sjulia_doc_<Name> = "<doc>"` main statement (Issue #10164) that
        // registers the docstring for `@doc` — it is NOT a user-typed value
        // expression. Treat a `main` whose only statements are such doc
        // registrations as still definition-only, so a documented
        // `struct`/`abstract type`/method definition returns `Nothing` (like the
        // undocumented form) instead of echoing the docstring value. The doc
        // registration statements STAY in `main` and still run — this only
        // affects the REPL echo decision, not registration.
        let user_main_empty = program
            .main
            .stmts
            .iter()
            .all(crate::lowering::is_doc_registration_stmt);
        // Capture BEFORE `merge_definitions` appends prior methods. Source-order
        // world activation applies only to methods written in THIS input; prior
        // methods were already executed and must remain visible in fresh full
        // recompiles (Issues #9650/#9787).
        let current_input_program_function_count = program.functions.len();
        let current_input_function_count = repl_support::source_function_count(&program);
        let current_input_stored_function_count = current_input_function_count
            + repl_support::collect_main_inline_named_functions(&program).len();
        let current_input_struct_count = program.structs.len();
        let current_input_using_count = program.usings.len();
        let current_input_type_names = repl_support::current_type_names(&program);
        let current_input_runtime_nominal_names =
            repl_support::current_runtime_nominal_names(&program);
        if let Some(name) = self.prior_nominal_redefinition(&program) {
            return REPLResult::error(
                format!("Type error: invalid redefinition of constant {name}"),
                String::new(),
            );
        }
        let (
            current_input_module_positions,
            current_input_module_function_positions,
            current_input_module_binding_positions,
        ) = collect_module_recovery_source_positions(&program.modules);
        // The only compile-time use of a possible-assignment scan is the
        // transitional full-rebuild hazard where a struct-bearing input also
        // self-rebinds a prior global. Runtime persistence never consumes this
        // conservative set (Issue #9784/#11546).
        let struct_seed_rebindings = if current_input_struct_count == 0 {
            std::collections::HashSet::new()
        } else {
            let candidates = self.globals.variable_names().into_iter().collect();
            potential_rebindings_of(&program.main.stmts, &candidates)
        };
        let mut current_input_nonvalue_binding_names: std::collections::HashSet<String> = program
            .functions
            .iter()
            .map(|function| function.name.clone())
            .chain(
                program
                    .structs
                    .iter()
                    .map(|definition| definition.name.clone()),
            )
            .chain(
                program
                    .abstract_types
                    .iter()
                    .map(|definition| definition.name.clone()),
            )
            .chain(
                program
                    .primitive_types
                    .iter()
                    .map(|definition| definition.name.clone()),
            )
            .chain(program.modules.iter().map(|module| module.name.clone()))
            .chain(
                program
                    .macros
                    .iter()
                    .map(|definition| definition.name.clone()),
            )
            .collect();
        let mut current_input_enum_definitions = Vec::new();
        collect_main_enum_defs(&program.main, &mut current_input_enum_definitions);
        current_input_nonvalue_binding_names.extend(
            current_input_enum_definitions
                .iter()
                .map(|definition| definition.name.clone()),
        );
        let current_input_is_definition_free = program.functions.is_empty()
            && program.structs.is_empty()
            && program.abstract_types.is_empty()
            && program.primitive_types.is_empty()
            && program.enums.is_empty()
            && program.modules.is_empty()
            && program.macros.is_empty()
            && program.type_aliases.is_empty()
            && program.usings.is_empty();
        // Before Issue #11569, any hard-scope input forced a fresh VM and its
        // reachable-only transplant compacted the carried struct heap. Preserve
        // that bounded-memory boundary when the same input runs transactionally
        // on the parked VM: after success, compact in place before projecting
        // globals and the return value back into session state.
        let input_has_hard_scope = repl_support::program_main_has_hard_scope_block(&program);

        // Qualified paths of every module the USER (re)defined in THIS input,
        // captured BEFORE `merge_definitions` re-folds prior modules and BEFORE
        // the package loader appends `using X` modules. A module here is being
        // realized this eval, so its `__init__` must run now; a prior module NOT
        // in this set has already been realized and its re-run `__init__` is
        // suppressed under the persistent model (Issue #9199 S4). Package modules
        // never appear here, so they keep re-running on the fresh full-recompile path.
        let mut newly_defined_module_paths = std::collections::HashSet::new();
        collect_module_paths(&program.modules, "", &mut newly_defined_module_paths);

        // Issue #9199 S5 — input-delta compile. Under the persistent model, an
        // input that only APPENDS brand-new definitions/expressions (no
        // redefinition, base extension, structs, modules, macros, `using`, or
        // opaque `eval`) is compiled against the accumulated program via
        // `repl_delta_compile` instead of re-merging + recompiling the whole
        // session — making per-eval compile cost independent of session length
        // (the epic's exit criterion). Decided BEFORE `program` is mutated. Any
        // Ineligible input takes the full path below, which also refreshes the
        // persistent compile cache so later appends can
        // resume the delta path. See `docs/vm/ADR_REPL_EVAL_MODEL.md`.
        // Definition appends advance `persistent_compile` transactionally after
        // the live run succeeds, so the compiler prefix and live function/type
        // registries remain aligned across consecutive deltas (Issue #9784).
        let delta_eligible =
            self.persistent_compile.is_some() && self.persistent_delta_eligible(&program);

        // Per-eval RNG seed, hoisted so the fresh-build path and the LV2
        // live-append path share it so path selection does not change the seeded
        // random sequence pinned by the golden corpus.
        let eval_seed = self.seed.wrapping_add(self.eval_count);
        self.eval_count += 1;

        // LV2/LV5 (Issue #9199): the live-append fast path runs when an eligible
        // delta meets a HELD live VM. `persistent_delta_eligible` already decided
        // module eligibility — a pure-global/function/struct delta always
        // qualifies, and a module-referencing delta qualifies only for a session
        // whose modules are simple persistable user modules (their state lives IN
        // the live VM, so re-entering it preserves the #5296 mutable const state
        // without `restore_module_globals`). A module-bearing session has NO
        // fresh-VM delta path (the fresh VM lacks the realized module), so it MUST
        // use the live path here or fall back to the full recompile below.
        let live_delta_eligible = delta_eligible && self.live_vm.is_some();

        // Time the whole compile phase (merge + global fold + codegen), the
        // quantity the epic's exit criterion tracks (Issue #9199 S5).
        let compile_start = std::time::Instant::now();

        let prior_global_names: std::collections::HashSet<String>;
        let main_scope_names: std::collections::HashSet<String>;
        // Fresh accumulated compile cache to install after a successful full
        // compile. `None` means the live/delta path reused the existing prefix.
        let mut pending_persistent: Option<repl_support::ReplPersistentCompile> = None;
        // A live enum declaration mutates a thread-local formatting/construction
        // registry while it runs. Keep its rollback guard alive until the same
        // runtime/compiler transaction is accepted or rejected (Issue #9784).
        let mut enum_registry_transaction: Option<EnumRegistryTransaction> = None;
        let mut vm: Vm<StableRng>;
        // LV2 live-append fast path (Issue #9199): compile ONLY the new input as a
        // relocatable delta main (global slots seeded from the live VM's frame-0),
        // splice it onto the held live VM, and re-enter — preserving frame-0
        // globals / struct heap / dispatch caches / world so per-eval cost stops
        // scaling with session length. Returns `None` (fall through to the fresh
        // build below) whenever the delta is not cleanly appendable (a lifted
        // lambda/closure, a preload splice, a slot-seed mismatch, ...).
        let live_prepared = if live_delta_eligible {
            self.try_live_delta_run(&program, eval_seed)
        } else {
            None
        };
        let ran_live_delta = live_prepared.is_some();
        // A plain live delta expects an empty definition prefix. Definition
        // deltas carry one typed, interleaved function/type chronology; the VM
        // proves the exact reached prefix before any compiler/session snapshot
        // is projected or committed (Issues #9784/#11546).
        let mut recover_live_vm_after_error = live_prepared.as_ref().and_then(|prepared| {
            let appended_function_count = prepared.appended_function_names.len();
            let source_function_count = prepared.source_function_indices.len();
            let appended_struct_count = prepared.appended_struct_names.len();
            let appended_abstract_type_count = prepared.appended_abstract_type_names.len();
            let appended_primitive_type_count = prepared.appended_primitive_type_names.len();
            let appended_enum_count = prepared.appended_enum_names.len();
            let snapshot_stable = prepared.next_persistent.is_none()
                && appended_function_count == 0
                && appended_struct_count == 0
                && appended_abstract_type_count == 0
                && appended_primitive_type_count == 0
                && appended_enum_count == 0
                && prepared.definition_activations.is_empty()
                && prepared.runtime_nominal_templates.is_empty();
            let has_definitions = appended_function_count
                + appended_struct_count
                + appended_abstract_type_count
                + appended_primitive_type_count
                + appended_enum_count
                > 0
                || !prepared.runtime_nominal_templates.is_empty();
            let definition_prefix = prepared.next_persistent.is_some()
                && has_definitions
                && prepared.definition_activations.len()
                    == source_function_count
                        + appended_struct_count
                        + appended_abstract_type_count
                        + appended_primitive_type_count
                        + appended_enum_count;
            (snapshot_stable || definition_prefix).then(|| LiveErrorRecoveryPlan {
                first_appended_global_slot_index: prepared.first_appended_global_slot_index,
                first_appended_function_index: prepared.first_appended_function_index,
                appended_function_names: prepared.appended_function_names.clone(),
                source_function_indices: prepared.source_function_indices.clone(),
                runtime_function_qualified_names: HashMap::new(),
                first_appended_struct_index: prepared.first_appended_struct_index,
                appended_struct_names: prepared.appended_struct_names.clone(),
                first_appended_abstract_type_index: prepared.first_appended_abstract_type_index,
                appended_abstract_type_names: prepared.appended_abstract_type_names.clone(),
                first_appended_primitive_type_index: prepared.first_appended_primitive_type_index,
                appended_primitive_type_names: prepared.appended_primitive_type_names.clone(),
                first_appended_enum_index: prepared.first_appended_enum_index,
                appended_enum_names: prepared.appended_enum_names.clone(),
                definition_activations: prepared.definition_activations.clone(),
                runtime_nominal_templates: prepared.runtime_nominal_templates.clone(),
            })
        });
        // If the live append compiler rejected this input, take the conservative
        // full-refresh path. The fresh-delta compiler reuses the prefix and can
        // otherwise emit call sites with no candidate payload for methods that
        // are present only in the live VM / refreshed full program (Issue #9980).
        let live_delta_rejected = live_delta_eligible && live_prepared.is_none();

        if let Some(prepared) = live_prepared {
            self.last_compile_nanos = Some(compile_start.elapsed().as_nanos());
            // The live VM is reused in place — no fresh `Vm::new_program`.
            self.last_vm_build_nanos = Some(0);
            pending_persistent = prepared.next_persistent;
            enum_registry_transaction = prepared.enum_registry_transaction;
            vm = prepared.vm;
            main_scope_names = prepared.main_scope_names;
            // A definition delta carries an advanced compiler snapshot; a plain
            // expression/global delta leaves this `None` to avoid copying the
            // accumulated prefix on every eval.
            // Re-read EVERY prior global after the run (a called function's
            // `global x = …` mutation is captured), exactly like the fresh delta
            // path — the live VM's frame-0 is the source of truth, this only syncs
            // the session mirror used for display + the full-recompile fallback.
            prior_global_names = self.globals.variable_names().into_iter().collect();
        } else {
            let mut seed_globals: Vec<(String, Value)>;
            let compiled;
            // The fresh-VM S5 input-delta path (`repl_delta_compile`) handles only
            // EXPRESSION / global deltas — inputs that DEFINE nothing
            // (`program.functions.is_empty() && program.structs.is_empty()`). A
            // DEFINITION that reached this fall-through (its live-append was
            // ineligible or the extraction gate rejected it — a function LV3 or a
            // struct LV4) takes the FULL recompile path instead — which re-merges
            // every prior definition and REFRESHES the prefix (clearing staleness),
            // never the fresh delta path. The fresh delta path leaves
            // `persistent_compile` unchanged, so routing a definition through it
            // would silently strand the new function/struct out of the prefix and
            // mis-compile the NEXT eval's reference to it. `delta_eligible` already
            // excludes a stale prefix (Issue #9199 LV3/LV4).
            if delta_eligible
                && !live_delta_rejected
                && program.functions.is_empty()
                && program.structs.is_empty()
                && !self.has_stateful_user_module()
            {
                // Input-delta path: no `merge_definitions`, no package load, no
                // module restore/reinit — the accumulated compiled program already
                // holds every prior definition and the input adds no modules/usings.
                // Globals are still value-carried into the fresh VM (Issue #9199 S3).
                // A session with a stateful USER module is EXCLUDED (Issue #9199
                // LV5): the fresh VM does not re-run the module body, so the
                // module's realized const state (which lives only in the parked
                // live VM) would be missing — such a delta must take the live path
                // above or, if no live VM is held (e.g. after an error), the full
                // recompile below (which re-realizes the module + restores state).
                // Delta path: `merge_definitions` never ran, so no replayed
                // enum statements were spliced (offset 0).
                seed_globals = self.inject_globals(
                    &mut program,
                    0,
                    &struct_seed_rebindings,
                    current_input_struct_count > 0,
                );
                prior_global_names = seed_globals.iter().map(|(name, _)| name.clone()).collect();
                // INTERNAL: `delta_eligible` (computed above) short-circuits on
                // `self.persistent_compile.is_some()`, and nothing between that
                // snapshot and this line replaces `persistent_compile` with `None`
                // (`try_live_delta_run` only reads it via `.as_ref()` and never
                // clears it on its `None`-returning paths). So this is
                // structurally guaranteed today, not merely by comment — but a
                // guarded match keeps a future change to that invariant a typed
                // error instead of a host panic (Issue #10906, Phase 1c of #10869).
                let prev =
                    match self.persistent_compile.as_ref() {
                        Some(prev) => prev,
                        None => return REPLResult::error(
                            "internal error: delta_eligible implied a prior persistent compile, \
                             but none was found"
                                .to_string(),
                            String::new(),
                        ),
                    };
                match repl_support::delta_compile(
                    prev,
                    &program,
                    &self.global_types,
                    &self.global_struct_names,
                ) {
                    Ok((c, _state)) => {
                        // Keep `pending_persistent` = None: a delta reuses the last full
                        // compile's bundle as its fixed prefix and must NOT replace it
                        // (a delta adds no reusable functions; installing its bundle
                        // would grow the prefix — and the O(session) prefix copy —
                        // unboundedly across expression evals, Issue #9199 LV2).
                        compiled = c;
                    }
                    Err(e) => {
                        return REPLResult::error(format!("Compile error: {:?}", e), String::new())
                    }
                }
            } else {
                // Full accumulate-and-recompile fallback for an input that is not
                // yet safe for the live/delta path.
                // Merge with existing functions and structs.
                let replayed_prefix_stmts = self.merge_definitions(&mut program);

                let mut package_loader =
                    loader::PackageLoader::new(loader::LoaderConfig::from_env());
                if let Err(e) = package_loader.load_into_program(&mut program) {
                    return REPLResult::error(format!("Load error: {}", e), String::new());
                }
                // A full compile executes import publication stores alongside
                // user main. Their visible aliases and compiler-owned provenance
                // slots are binding metadata, not value writes to persist in
                // Main's REPL mirror (Issue #9784).
                current_input_nonvalue_binding_names
                    .extend(repl_support::main_import_binding_names(&program));
                current_input_nonvalue_binding_names
                    .extend(program.modules.iter().map(|module| module.name.clone()));

                // Bring prior globals into this fresh full-recompile eval. Simple
                // carriable values are seeded directly into the VM binding table;
                // heap-backed values that require source-level type reconstruction
                // are prepended as init statements, and values with no expressible
                // form are also seeded directly (Issue #8260). Compiler type hints
                // come from `global_types` / `global_struct_names`.
                seed_globals = self.inject_globals(
                    &mut program,
                    replayed_prefix_stmts,
                    &struct_seed_rebindings,
                    current_input_struct_count > 0,
                );

                // After the run, re-read every directly carried prior global from
                // the VM binding table. Runtime write provenance captures called-
                // function mutations and re-syncs StructRef indices after
                // reachable-heap compaction (Issues #9199, #9787).
                // Carried globals have no `main` assignment, so they must be listed
                // explicitly here.
                prior_global_names = seed_globals.iter().map(|(name, _)| name.clone()).collect();

                // Restore persisted module-level mutable state (e.g. Plots._CURRENT_SERIES)
                // by rewriting the relevant `const` initializers in the re-injected module
                // bodies. Without this, module bodies re-run and reset their state every
                // eval, so `plot!` could not append to the current plot (Issue #5296).
                // Modules the CURRENT input (re)defines are skipped: upstream module
                // REDEFINITION replaces the binding, so the new module starts from its
                // OWN initializers — restoring the old module's persisted state into it
                // resumed the old state, diverging from upstream (Issue #10232).
                self.restore_module_globals(&mut program, &newly_defined_module_paths);

                // Module once-initialization (Issue #9199 S4). Under the persistent model,
                // empty the `__init__` body of every already-realized user module that the
                // current input did NOT (re)define, so its init side effects fire once (at
                // definition) instead of on every accumulate-and-recompile pass. Upstream
                // realizes a module — and calls its `__init__` — exactly once
                // (`run_module_init`, `julia/base/loading.jl`); the historical
                // per-eval re-fire is the observable divergence this fixes. Module BODY const bindings still
                // re-run and are persisted by `restore_module_globals` above, so a call-
                // driven mutable module global (`Log.entries`) is preserved either way;
                // only the `__init__` re-fire is suppressed.
                self.suppress_module_reinit(&mut program, &newly_defined_module_paths);

                // Resolve struct type_ids from struct_names before compilation
                // This is needed because VM's type_id may not match compile-time struct_table indices
                let resolved_global_types = self.global_types.clone();
                // Note: We'll resolve struct_names to type_ids in compile_core_program_with_globals
                // after struct_table is built

                // Capture the reusable compile bundle so subsequent delta-safe
                // inputs can append against it (Issue #9199 S5).
                match repl_support::full_compile(
                    &program,
                    &resolved_global_types,
                    &self.global_struct_names,
                    current_input_function_count,
                    current_input_struct_count,
                    &current_input_type_names,
                    &current_input_runtime_nominal_names,
                ) {
                    Ok((c, state)) => {
                        compiled = c;
                        pending_persistent = Some(state);
                    }
                    Err(e) => {
                        return REPLResult::error(format!("Compile error: {:?}", e), String::new())
                    }
                }
            }

            // An executed qualified StoreGlobalAny may create a module binding
            // with no module-body assignment to rewrite on the next full rebuild
            // (for example a stdlib intrinsic installed from a called function).
            // Carry those values directly under their qualified names, but never
            // seed bindings owned by a module the current input redefines: the new
            // module object must start empty except for its own body (Issue #9784).
            seed_globals.extend(
                self.module_runtime_global_names
                    .iter()
                    .filter(|name| {
                        !binding_belongs_to_any_module(name, &newly_defined_module_paths)
                    })
                    .filter_map(|name| {
                        self.module_globals
                            .get(name)
                            .cloned()
                            .map(|value| (name.clone(), value))
                    }),
            );

            // Record the compile-phase cost for this eval (Issue #9199 S5).
            self.last_compile_nanos = Some(compile_start.elapsed().as_nanos());

            // Snapshot which names are genuinely bound at main/module scope once
            // this eval finished compiling (Issue #9157/#9182). A top-level
            // `let`/`@testset` block restores the compiler's `initialized_locals`
            // on exit, so a brand-new local it introduced is ABSENT here even
            // though its main-frame slot value can survive optimization — that
            // stale slot is exactly what the scope-blind `Vm::get_global`
            // persistence below would otherwise leak into the next eval.
            // `begin`/`for`/`while`/`if` do NOT unwind, so their names stay in this
            // set and keep persisting (Issues #9156/#9157). Captured before
            // `compiled` is moved into the VM.
            main_scope_names = compiled.main_scope_names.clone();
            let fresh_definition_recovery = full_compile_publication_recovery_plan(
                &compiled,
                current_input_stored_function_count,
                current_input_using_count,
                newly_defined_module_paths.len(),
            );

            let rng = StableRng::new(eval_seed);
            // Time VM construction separately from the compile phase (Issue #9199
            // live-VM slice). `Vm::new_program` re-derives every program-scaled
            // table (`call_site_caches`, `ExecutableProgram`, per-function
            // slot/name maps, type ancestry) from the WHOLE accumulated program
            // each eval. Measuring it apart from compile lets the exit-criterion
            // benchmark locate the epic's O(session) cost — which the measurement
            // puts in the COMPILE phase, this vm-build phase staying small and
            // ~flat (Base-dominated) in the practical N range. See
            // `docs/vm/ADR_REPL_EVAL_MODEL.md` §"Live-VM slice decomposition".
            let vm_build_start = std::time::Instant::now();
            vm = Vm::new_program(compiled, rng);
            self.last_vm_build_nanos = Some(vm_build_start.elapsed().as_nanos());
            // Route `display(x)` through the host artifact channel when this
            // session runs under a graphical host (Issue #9262).
            if self.graphical_display {
                vm.enable_graphical_display();
            }
            if recover_live_vm_after_error.is_none()
                && fresh_definition_recovery.is_some()
                && pending_persistent.is_some()
            {
                recover_live_vm_after_error = fresh_definition_recovery;
            }

            // Seed globals that could not be reconstructed as init statements by
            // transplanting the prior eval's struct heap and binding the real
            // runtime `Value` directly into the fresh VM's module scope (Issue
            // #8260). Done before `run()` so the injected program body can read
            // these globals.
            if !seed_globals.is_empty() {
                // Reachable-only transplant compaction (Issue #9787): transplant
                // ONLY the structs a carried global can reach, not the whole
                // accumulated `last_struct_heap` (which grew without bound across a
                // long session, violating the #8625 guarantee). Compaction densifies
                // the heap, so a carried global's `StructRef` moves (e.g. 33→1);
                // `reachable_compacted_struct_heap` remaps `seed_globals` (the values
                // handed to the VM) to match the compacted heap. The session's OWN
                // cached indices (`self.globals`) are kept consistent NOT by a
                // fragile pre-run in-place remap (which double-remaps shared `Rc`
                // containers and is not transactional if the run errors) but by
                // re-reading every carried global from the VM after a successful run
                // (`extract_globals_from_vm` via `prior_global_names` below) — the VM
                // is the source of truth for the post-run indices. On an errored run
                // `self.globals` and `last_struct_heap` are both left untouched, so
                // they stay mutually consistent (Issue #9787).
                let (mut transplant_heap, _remap) = vm_repl::reachable_compacted_struct_heap(
                    &self.last_struct_heap,
                    &mut seed_globals,
                );
                if let Some(prior) = self.persisted_callable_snapshot.as_ref() {
                    vm.remap_persisted_callable_candidates_from(
                        prior,
                        &mut seed_globals,
                        &mut transplant_heap,
                    );
                }
                vm.seed_persisted_globals(seed_globals, transplant_heap);
            }
        }

        let definition_world_before_run = recover_live_vm_after_error
            .is_some()
            .then(|| vm.repl_definition_world_fingerprint());
        // Refresh the callable identity table only when the program layout can
        // change. Append-only live deltas extend just the suffix; full rebuilds
        // replace the table. Plain live expressions do neither.
        let refresh_callable_snapshot_on_success = !ran_live_delta || pending_persistent.is_some();

        match vm.run() {
            Ok(value) => {
                // Decide the user-visible result before projecting VM state. A
                // successful hard-scope live transaction compacts the parked VM
                // below, and `ans` is one of its frame-0 roots: replace that root
                // first so a stale prior result cannot keep an otherwise-dead
                // struct alive for one extra transaction.
                let new_functions: Vec<&Function> = program
                    .functions
                    .iter()
                    .map(|f| f.as_ref())
                    .filter(|f| {
                        !is_internal_lowered_function(f)
                            && !self
                                .functions
                                .iter()
                                .any(|existing| existing.name == f.name)
                    })
                    .collect();
                let mut return_value = if import_only_input {
                    Value::Nothing
                } else if new_functions.len() == 1 {
                    Value::Function(FunctionValue::new(new_functions[0].name.clone()))
                } else if user_main_empty {
                    Value::Nothing
                } else {
                    value
                };
                if ran_live_delta && input_has_hard_scope {
                    if !matches!(return_value, Value::Nothing) {
                        vm.store_global_value("ans", return_value.clone());
                    }
                    vm.compact_struct_heap_at_safe_point_with_return(Some(&mut return_value));
                }
                let reached_definition_prefix = definition_world_before_run
                    .zip(recover_live_vm_after_error.as_ref())
                    // A successful method-only full compile commits through the
                    // ordinary `store_definitions` path below. Its recovery plan
                    // exists solely for the error arm; success-prefix validation
                    // remains necessary for live deltas and runtime-nominal
                    // compiler adoption (Issue #11742).
                    .filter(|(_, plan)| {
                        ran_live_delta || !plan.runtime_nominal_templates.is_empty()
                    })
                    .map(|(before, plan)| {
                        vm.repl_reached_appended_definition_prefix(
                            before,
                            &plan.definition_activations,
                            &plan.runtime_nominal_templates,
                            ReplAppendDefinitionStarts {
                                functions: plan.first_appended_function_index,
                                structs: plan.first_appended_struct_index,
                                abstract_types: plan.first_appended_abstract_type_index,
                                primitive_types: plan.first_appended_primitive_type_index,
                                enums: plan.first_appended_enum_index,
                            },
                            ReplAppendDefinitionCounts {
                                function_bodies: plan.appended_function_names.len(),
                                source_functions: plan.source_function_indices.len(),
                                structs: plan.appended_struct_names.len(),
                                abstract_types: plan.appended_abstract_type_names.len(),
                                primitive_types: plan.appended_primitive_type_names.len(),
                                enums: plan.appended_enum_names.len(),
                            },
                            &plan.source_function_indices,
                        )
                        .filter(|reached| {
                            reached.function_count == plan.source_function_indices.len()
                                && reached.struct_count == plan.appended_struct_names.len()
                                && reached.abstract_type_count
                                    == plan.appended_abstract_type_names.len()
                                && reached.primitive_type_count
                                    == plan.appended_primitive_type_names.len()
                                && reached.enum_count == plan.appended_enum_names.len()
                        })
                    });
                let reached_definition_prefix = match reached_definition_prefix {
                    Some(Some(reached)) => Some(reached),
                    Some(None) => {
                        return REPLResult::error(
                            "internal error: live definition activation trace diverged".to_string(),
                            vm.get_output().to_string(),
                        );
                    }
                    None => None,
                };
                if let Some(reached) = reached_definition_prefix.as_ref().filter(|_| {
                    recover_live_vm_after_error
                        .as_ref()
                        .is_some_and(|plan| !plan.runtime_nominal_templates.is_empty())
                }) {
                    let Some(state) = pending_persistent.take().and_then(|state| {
                        state
                            .adopt_runtime_nominal_activations(&reached.runtime_nominal_activations)
                    }) else {
                        return REPLResult::error(
                            "internal error: runtime nominal compiler adoption diverged"
                                .to_string(),
                            vm.get_output().to_string(),
                        );
                    };
                    pending_persistent = Some(state);
                }
                if let Some(transaction) = enum_registry_transaction.take() {
                    transaction.commit();
                }

                // Capture every artifact emitted by explicit `display(x)` calls
                // during this eval before the VM is further inspected (Issue #9262).
                let mut display_artifacts = vm.take_display_artifacts();
                // Capture VM memory/cache counters for long-session
                // observability (Issue #8625). Taken after run() so it reflects
                // any safe-point compaction / hard-cap clears from this eval.
                self.last_vm_memory_stats = Some(vm.memory_stats());
                let output = vm.get_output().to_string();

                // Executed VM stores are the sole value-binding authority. The
                // main-scope intersection excludes hard-scope locals, while
                // explicit-global provenance admits writes performed inside a
                // called function or `global` declaration (Issue #9784).
                let committed_value_rebindings = runtime_value_rebindings(
                    vm.repl_written_global_names(),
                    vm.repl_explicit_global_write_names(),
                    &main_scope_names,
                    &current_input_nonvalue_binding_names,
                    &current_input_alias_names,
                );
                let mut authoritative_globals = prior_global_names.clone();
                authoritative_globals.extend(committed_value_rebindings.iter().cloned());
                self.extract_globals_from_vm(&vm, &authoritative_globals);

                // Store VM's struct heap for resolving StructRefs in display
                self.last_struct_heap = vm.get_struct_heap().to_vec();

                // Capture module-level mutable state so it survives into the next
                // eval (module bodies otherwise re-initialize it). Done after
                // last_struct_heap is set, since captured values may hold StructRefs
                // that resolve against that heap (Issue #5296).
                self.extract_module_globals_from_vm(&vm, &program);

                // Store result in ans
                if !matches!(return_value, Value::Nothing) {
                    self.ans = Some(return_value.clone());
                    // `ans` is written here, NOT through `extract_globals_from_vm`,
                    // so its compiler type hint (`global_types["ans"]`) must be kept
                    // in sync here or it goes stale. `ans` is value-carried with no
                    // init statement, so a stale hint mis-compiles the next eval's `ans`
                    // load — e.g. a leftover `Str` hint reads "" for an `Int`
                    // (Issue #9199).
                    self.record_ans_type(&return_value);
                    self.globals.set("ans", return_value.clone());
                }

                // Store new function and struct definitions
                let mut current_enum_definitions = Vec::new();
                collect_main_enum_defs(&program.main, &mut current_enum_definitions);
                self.store_definitions(
                    &program,
                    &committed_value_rebindings,
                    current_input_stored_function_count,
                    Some(current_input_program_function_count),
                    program.structs.len(),
                    program.abstract_types.len(),
                    program.primitive_types.len(),
                    current_enum_definitions.len(),
                    true,
                );
                if let Some(reached) = reached_definition_prefix.as_ref() {
                    self.store_runtime_nominal_activations(&reached.runtime_nominal_activations);
                }

                // Install the accumulated compile cache produced this eval so the
                // NEXT delta-safe input compiles only against it (Issue #9199 S5).
                // `None` on live/delta reuse leaves the existing cache untouched.
                // Done after a successful run so an error does not advance the cache.
                // A full recompile or successful definition append supplies an
                // advanced prefix; expression/global reuse leaves it untouched.
                if let Some(state) = pending_persistent {
                    self.persistent_compile = Some(state);
                }

                // Record every user module realized this eval so its `__init__`
                // is suppressed on subsequent evals under the persistent model
                // (Issue #9199 S4). Recorded after a successful run so a failed
                // module definition does not count as realized; a later
                // redefinition re-enters `newly_defined_module_paths` and so
                // re-runs `__init__`, matching upstream module redefinition.
                self.initialized_module_paths
                    .extend(newly_defined_module_paths.iter().cloned());

                // Prefer artifacts produced by explicit `display(x)` calls this
                // eval (Issue #9262); otherwise auto-generate one when the result
                // value is itself a Plot struct.
                if display_artifacts.is_empty() {
                    if let Some(artifact) = crate::plotting::try_value_to_artifact(
                        &return_value,
                        &self.last_struct_heap,
                    ) {
                        display_artifacts.push(artifact);
                    }
                }

                // Render the result through its user-defined `show` method (if any)
                // so the REPL/FFI echo matches `string(x)` instead of dumping
                // struct fields (Issue #7168). Safe to run after `run()` returned:
                // the VM state is intact and `show` is read-only on globals/heap.
                let value_display = if matches!(return_value, Value::Nothing) {
                    None
                } else {
                    vm.render_value_via_user_show(&return_value)
                };

                // Hold this VM live for the next eval's LV1 append: its frame-0
                // globals / struct heap / dispatch caches /
                // world are exactly what an expression-only delta must observe
                // WITHOUT re-seeding (Issue #9199 LV1). Done after a successful
                // run so an errored eval never leaves a half-run VM live.
                // Hard-scope locals now live in an explicit VM lexical
                // environment, so leaving a `let`/loop/testset no longer clears
                // or aliases frame-0 slots (Issue #11569 / #9784). The completed
                // VM is therefore the authoritative transaction and is parked
                // exactly like every other successful eval.
                //
                // `ans` is session-managed (set above, not written by the VM
                // main), so mirror the CURRENT `ans` into this VM's frame-0
                // before parking it — otherwise the next LV1 delta would read a
                // stale `ans` from a prior eval. `ans` came from THIS run, so its
                // heap refs are already valid in this VM's heap (no transplant
                // needed — unlike `seed_persisted_globals`).
                if let Some(ans) = self.ans.clone() {
                    vm.store_global_value("ans", ans);
                }
                if refresh_callable_snapshot_on_success {
                    self.refresh_persisted_callable_snapshot(&vm, ran_live_delta);
                }
                self.live_vm = Some(vm);

                let mut result = REPLResult::success(return_value, output);
                result.display_artifacts = display_artifacts;
                result.value_display = value_display;
                result
            }
            Err(e) => {
                let output = vm.get_output().to_string();
                let error_source_position = vm
                    .last_error_span()
                    .or_else(|| vm.last_error_callsite_span())
                    .map(|span| span.start);
                let mut reached_module_paths = vm
                    .repl_reached_module_activations()
                    .iter()
                    .filter(|path| newly_defined_module_paths.contains(*path))
                    .cloned()
                    .collect::<std::collections::HashSet<_>>();
                // Validate the runtime boundary first, then derive the compiler
                // checkpoint before mutating any host/session mirror. A plain
                // expression delta yields `(0, None)`; a definition delta yields
                // an advanced snapshot with only its reached method prefix
                // published. Any mismatch fails closed and drops the live VM.
                let recovery_checkpoint = definition_world_before_run
                    .zip(recover_live_vm_after_error.as_ref())
                    .and_then(|(before, plan)| {
                        vm.repl_reached_appended_definition_prefix(
                            before,
                            &plan.definition_activations,
                            &plan.runtime_nominal_templates,
                            ReplAppendDefinitionStarts {
                                functions: plan.first_appended_function_index,
                                structs: plan.first_appended_struct_index,
                                abstract_types: plan.first_appended_abstract_type_index,
                                primitive_types: plan.first_appended_primitive_type_index,
                                enums: plan.first_appended_enum_index,
                            },
                            ReplAppendDefinitionCounts {
                                function_bodies: plan.appended_function_names.len(),
                                source_functions: plan.source_function_indices.len(),
                                structs: plan.appended_struct_names.len(),
                                abstract_types: plan.appended_abstract_type_names.len(),
                                primitive_types: plan.appended_primitive_type_names.len(),
                                enums: plan.appended_enum_names.len(),
                            },
                            &plan.source_function_indices,
                        )
                        .and_then(|reached| {
                            let source_frontier = error_source_position.or_else(|| {
                                reached
                                    .runtime_nominal_activations
                                    .iter()
                                    .map(|activation| activation.span.start)
                                    .max()
                            });
                            reached_module_paths.retain(|path| {
                                current_input_module_positions
                                    .get(path)
                                    .is_some_and(|position| {
                                        source_frontier.is_none_or(|frontier| *position <= frontier)
                                    })
                            });
                            if reached_module_paths.is_empty()
                                && plan.appended_function_names.is_empty()
                                && plan.appended_struct_names.is_empty()
                                && plan.appended_abstract_type_names.is_empty()
                                && plan.appended_primitive_type_names.is_empty()
                                && plan.appended_enum_names.is_empty()
                                && plan.runtime_nominal_templates.is_empty()
                            {
                                Some((reached, None))
                            } else {
                                pending_persistent.take().and_then(|state| {
                                    state
                                        .retain_reached_function_prefix(
                                            plan.first_appended_function_index,
                                            &plan.appended_function_names,
                                            &plan.source_function_indices,
                                            reached.function_count,
                                            &plan.definition_activations,
                                            &reached.runtime_constructor_indices,
                                        )
                                        .and_then(|state| {
                                            state.retain_reached_struct_prefix(
                                                plan.first_appended_struct_index,
                                                &plan.appended_struct_names,
                                                reached.struct_count,
                                            )
                                        })
                                        .and_then(|state| {
                                            state.retain_reached_nominal_prefixes(
                                                plan.first_appended_abstract_type_index,
                                                &plan.appended_abstract_type_names,
                                                reached.abstract_type_count,
                                                plan.first_appended_primitive_type_index,
                                                &plan.appended_primitive_type_names,
                                                reached.primitive_type_count,
                                                plan.first_appended_enum_index,
                                                &plan.appended_enum_names,
                                                reached.enum_count,
                                            )
                                        })
                                        .and_then(|state| {
                                            state.adopt_runtime_nominal_activations(
                                                &reached.runtime_nominal_activations,
                                            )
                                        })
                                        .map(|state| (reached, Some(state)))
                                })
                            }
                        })
                    });
                // Only ordinary Julia exceptions leave the VM at a defined
                // recoverable toplevel boundary. Host cancellation and internal
                // invariant failures are deliberately uncatchable; retaining
                // that VM could preserve corrupted transient state.
                let reached_using_indices = validate_reached_using_indices(
                    vm.repl_reached_using_activations(),
                    program.usings.len(),
                );
                if let Some(((reached, checkpointed_persistent), reached_using_indices)) =
                    recovery_checkpoint
                        .zip(reached_using_indices)
                        .filter(|_| e.is_catchable())
                {
                    let refresh_callable_snapshot = checkpointed_persistent.is_some();
                    let has_unreached_usings = reached_using_indices.len() < program.usings.len();

                    // The VM owns recovery of invocation-local state. Mutations
                    // completed before the exception remain in frame 0 / the
                    // heap, matching upstream toplevel eval semantics (#9784).
                    vm.discard_unreached_repl_struct_defs();
                    vm.recover_repl_toplevel_after_error(StableRng::new(eval_seed));
                    self.last_vm_memory_stats = Some(vm.memory_stats());
                    let reached_value_rebindings = runtime_value_rebindings(
                        vm.repl_written_global_names(),
                        vm.repl_explicit_global_write_names(),
                        &main_scope_names,
                        &current_input_nonvalue_binding_names,
                        &current_input_alias_names,
                    );
                    let mut authoritative_globals = prior_global_names.clone();
                    authoritative_globals.extend(reached_value_rebindings.iter().cloned());
                    let unobserved_function_only_append = ran_live_delta
                        && recover_live_vm_after_error.as_ref().is_some_and(|plan| {
                            !plan.appended_function_names.is_empty()
                                && plan.appended_struct_names.is_empty()
                                && plan.appended_abstract_type_names.is_empty()
                                && plan.appended_primitive_type_names.is_empty()
                                && plan.appended_enum_names.is_empty()
                                && plan.runtime_nominal_templates.is_empty()
                        })
                        && reached.function_count == 0
                        && reached.struct_count == 0
                        && reached.abstract_type_count == 0
                        && reached.primitive_type_count == 0
                        && reached.enum_count == 0;

                    // Keep the transitional full-recompile fallback coherent:
                    // if a later ineligible input still reconstructs a VM, it
                    // must see bindings and module mutations committed before
                    // this exception rather than the pre-error mirror values.
                    self.extract_globals_from_vm(&vm, &authoritative_globals);
                    // `ans` is a separate host-facing mirror and may contain a
                    // StructRef remapped by an explicit GC before this error.
                    // Refresh it from the recovered frame-0/heap pair.
                    self.last_struct_heap = vm.get_struct_heap().to_vec();
                    if let Some(ans) = vm.get_global("ans") {
                        self.ans = Some(ans.clone());
                        self.globals.set("ans", ans.clone());
                        self.record_ans_type(&ans);
                    }
                    self.extract_module_globals_from_vm(&vm, &program);
                    let recovered_module_replay =
                        recover_live_vm_after_error.as_ref().map(|plan| {
                            RecoveredModuleReplay::from_reached(
                                plan,
                                &reached,
                                &newly_defined_module_paths,
                                &reached_module_paths,
                                &current_input_module_function_positions,
                                &current_input_module_binding_positions,
                                &self.module_globals,
                            )
                        });
                    let rolled_back_unobserved_function_only_append =
                        unobserved_function_only_append
                            && recover_live_vm_after_error.as_ref().is_some_and(|plan| {
                                vm.rollback_unobserved_repl_function_append(
                                    plan.first_appended_function_index,
                                    plan.first_appended_global_slot_index,
                                )
                            });
                    if let Some(state) = checkpointed_persistent
                        .filter(|_| !rolled_back_unobserved_function_only_append)
                    {
                        if let Some(plan) = recover_live_vm_after_error.as_ref() {
                            let reached_enum_names = plan.appended_enum_names[..reached.enum_count]
                                .iter()
                                .cloned()
                                .collect();
                            record_observed_enum_member_publication(
                                &mut program.main,
                                &reached_enum_names,
                                &vm,
                            );
                        }
                        // Retain only the source definitions whose activation
                        // markers ran before the exception. The compiler snapshot
                        // keeps the dormant suffix solely for live index alignment;
                        // a later full fallback reconstructs from this reached IR
                        // prefix and therefore never revives it (Issue #9784).
                        self.store_definitions(
                            &program,
                            &reached_value_rebindings,
                            reached.function_count,
                            Some(current_input_program_function_count),
                            reached.struct_count,
                            reached.abstract_type_count,
                            reached.primitive_type_count,
                            reached.enum_count,
                            false,
                        );
                        self.store_runtime_nominal_activations(
                            &reached.runtime_nominal_activations,
                        );
                        if let Some(replay) = recovered_module_replay.as_ref() {
                            self.store_recovered_modules(&program.modules, replay);
                        }
                        self.persistent_compile = Some(state);
                    }
                    // Expression-only recovery has no compiler snapshot to
                    // store, while rollback may deliberately discard one.
                    // Runtime-observed value writes are authoritative in every
                    // recovery shape, so invalidate shadowed static aliases at
                    // this common boundary (#9784).
                    self.invalidate_type_aliases_for_value_rebindings(&reached_value_rebindings);
                    self.store_reached_usings(&program.usings, &reached_using_indices);
                    if let Some(transaction) = enum_registry_transaction.take() {
                        transaction.commit();
                    }
                    if refresh_callable_snapshot
                        && !rolled_back_unobserved_function_only_append
                        && !has_unreached_usings
                    {
                        self.refresh_persisted_callable_snapshot(&vm, true);
                    }
                    // Compiling an import reserves its static binding surface
                    // before runtime activation. If an import marker did not
                    // execute, retaining this VM would let `isdefined` observe
                    // that dormant source-later binding. The host/session mirror
                    // above contains the exact reached transaction, so force the
                    // next eval to rebuild from it instead (Issue #11748).
                    if !has_unreached_usings && reached_module_paths.is_empty() {
                        self.live_vm = Some(vm);
                    }
                } else if e.is_catchable()
                    && !ran_live_delta
                    && current_input_is_definition_free
                    && pending_persistent.is_some()
                {
                    // A conservative live-append rejection can route a pure
                    // expression through a fresh accumulated VM. Its prior
                    // definitions were replayed before this input and the input
                    // cannot change the method/type world, so the catchable
                    // toplevel boundary is still an exact persistent checkpoint.
                    // Preserve mutations completed before the exception and park
                    // this VM; dropping it would lose Rc-backed updates on the
                    // next failed eval (Issue #9784).
                    vm.recover_repl_toplevel_after_error(StableRng::new(eval_seed));
                    self.last_vm_memory_stats = Some(vm.memory_stats());
                    let reached_value_rebindings = runtime_value_rebindings(
                        vm.repl_written_global_names(),
                        vm.repl_explicit_global_write_names(),
                        &main_scope_names,
                        &current_input_nonvalue_binding_names,
                        &current_input_alias_names,
                    );
                    let mut authoritative_globals = prior_global_names.clone();
                    authoritative_globals.extend(reached_value_rebindings.iter().cloned());
                    self.extract_globals_from_vm(&vm, &authoritative_globals);
                    self.last_struct_heap = vm.get_struct_heap().to_vec();
                    if let Some(ans) = vm.get_global("ans") {
                        self.ans = Some(ans.clone());
                        self.globals.set("ans", ans.clone());
                        self.record_ans_type(&ans);
                    }
                    self.extract_module_globals_from_vm(&vm, &program);
                    self.persistent_compile = pending_persistent.take();
                    self.refresh_persisted_callable_snapshot(&vm, false);
                    self.live_vm = Some(vm);
                } else {
                    // A definition-bearing live delta may already have mutated a
                    // method/type world that could not be projected to the exact
                    // compiler boundary. Never retain a mismatched VM.
                    self.live_vm = None;
                }
                REPLResult::error(format!("{}", e), output)
            }
        }
    }

    /// Merge existing function, struct, and module definitions into the program.
    /// Whether the current input may take the input-delta compile path under the
    /// persistent model (Issue #9199 S5). Conservative by design: the input must
    /// define NO new methods/types/modules — only top-level expressions and/or
    /// global (re)assignments — so it cannot change how the ALREADY-compiled
    /// prefix dispatches or resolves. Any DEFINITION routes to the full recompile
    /// path, which recompiles every prior function against the new world; this is
    /// what keeps cross-eval dependencies correct WITHOUT precise per-name
    /// invalidation (deferred to S6 / #9197):
    ///
    /// - A forward reference (a function defined before its callee) is compiled to
    ///   a baked "Unknown function" trap in the caller. The full-recompile
    ///   fallback rebuilds the caller once the callee exists; a frozen delta prefix would
    ///   keep the trap. Routing the callee's DEFINITION through the full path
    ///   recompiles the caller, so a later expression that calls it (delta) reuses
    ///   the corrected body. Mutual recursion defined across two evals is the same
    ///   case. (Divergence observed and fixed via this gate — Issue #9199.)
    /// - Redefinition / method extension / Base extension likewise needs the prior
    ///   bodies rebuilt, so it must be a full recompile.
    ///
    /// Whether every function this input defines is an ordinary Main-owned
    /// method the compiled-definition live path can install (Issues #9199/#9784).
    /// Empty function
    /// set → trivially true (an expression/global delta). A function disqualifies
    /// the whole input (→ full recompile) when it is:
    /// - a **Base extension** (`Base.f(…)`) — extends an existing method table;
    /// - a same-name body not owned by the compiler's Main-source snapshot
    ///   (Base/preload/generated-only). Ordinary Main method extensions and
    ///   replacements — including `where` and keyword source methods — are
    ///   admitted; the compiler structurally verifies the complete aligned body
    ///   and specialization surface, appends the transitive caller refresh slice,
    ///   and publishes it with the method mutation.
    /// - compiler-generated anonymous helpers are admitted as callable values,
    ///   not Julia-visible generics. The relocatable compiler must structurally
    ///   prove their complete body/index closure and otherwise fails closed.
    fn input_defines_only_new_generic_functions(&self, program: &Program) -> bool {
        if program.functions.is_empty() {
            return true;
        }
        let Some(prev) = self.persistent_compile.as_ref() else {
            return false;
        };
        program.functions.iter().all(|f| {
            !f.is_base_extension
                && (is_internal_lowered_function(f)
                    || !prev.contains_function_body(&f.name)
                    || prev.owns_source_generic(&f.name))
        })
    }

    /// Reject a root declaration of a binding published by a runtime nominal
    /// site in an earlier REPL input. Runtime/root pairs inside the current
    /// fragment are not recorded until that fragment commits, so they remain
    /// eligible for Issue #11684's same-input identity coalescing. Root-only
    /// replay remains unchanged for existing long-session callers.
    fn prior_nominal_redefinition(&self, program: &Program) -> Option<String> {
        if let Some(definition) = program
            .structs
            .iter()
            .find(|definition| self.runtime_nominal_names.contains(&definition.name))
        {
            return Some(definition.name.clone());
        }
        if let Some(definition) = program
            .abstract_types
            .iter()
            .find(|definition| self.runtime_nominal_names.contains(&definition.name))
        {
            return Some(definition.name.clone());
        }
        if let Some(definition) = program
            .primitive_types
            .iter()
            .find(|definition| self.runtime_nominal_names.contains(&definition.name))
        {
            return Some(definition.name.clone());
        }
        let mut enum_definitions = Vec::new();
        collect_main_enum_defs(&program.main, &mut enum_definitions);
        enum_definitions
            .into_iter()
            .find(|definition| self.runtime_nominal_names.contains(&definition.name))
            .map(|definition| definition.name.clone())
    }

    /// Whether every TYPE this input DEFINES is a brand-new Main-owned nominal
    /// definition that the compiled live-append can install soundly (Issues
    /// #9199/#9784/#11635). Concrete structs retain their existing shape gates;
    /// abstract and primitive definitions use append-only metadata registries.
    ///   - NON-parametric (`type_params` empty) — a parametric struct is not
    ///     registered in `struct_defs` (it instantiates lazily per concrete
    ///     parameter), so it has no positional `type_id` to align (LV4b);
    ///   - WITHOUT inner constructors — an inner constructor generates helper
    ///     functions the front-of-fresh-region extraction cannot cleanly isolate
    ///     (LV4b);
    ///   - BRAND NEW — its name is absent from the reused prefix's TYPE names AND
    ///     FUNCTION names AND from prior user structs, so it is neither a
    ///     redefinition (which changes the baked `type_id` of every prior
    ///     `NewStruct` and MUST full-recompile) nor a struct/function name
    ///     collision.
    fn input_defines_only_new_types(&self, program: &Program) -> bool {
        let Some(prev) = self.persistent_compile.as_ref() else {
            return false;
        };
        let structs_are_new = program.structs.iter().all(|s| {
            !s.is_parametric()
                && s.inner_constructors.is_empty()
                && !prev.defines_type(&s.name)
                && !prev.defines_function(&s.name)
                && !self.structs.iter().any(|existing| existing.name == s.name)
        });
        let abstracts_are_new = program.abstract_types.iter().all(|definition| {
            !prev.defines_type(&definition.name)
                && !prev.defines_function(&definition.name)
                && !self
                    .abstract_types
                    .iter()
                    .any(|existing| existing.name == definition.name)
        });
        let primitives_are_new = program.primitive_types.iter().all(|definition| {
            !prev.defines_type(&definition.name)
                && !prev.defines_function(&definition.name)
                && !self
                    .primitive_types
                    .iter()
                    .any(|existing| existing.name == definition.name)
        });
        let mut enum_defs = Vec::new();
        collect_main_enum_defs(&program.main, &mut enum_defs);
        let enums_are_new = enum_defs.iter().all(|definition| {
            !prev.defines_type(&definition.name)
                && !prev.defines_function(&definition.name)
                && !self
                    .enums
                    .iter()
                    .any(|existing| existing.name == definition.name)
        });
        structs_are_new && abstracts_are_new && primitives_are_new && enums_are_new
    }

    /// Callers must have already checked `persistent_compile.is_some()`.
    fn persistent_delta_eligible(&self, program: &Program) -> bool {
        if self.persistent_compile.is_none() {
            return false;
        }
        // Module handling (Issue #9199 LV5). An input that DEFINES a module takes
        // the full recompile path, which realizes it on a fresh VM and parks that
        // VM; a subsequent module-REFERENCING delta re-enters the parked VM so the
        // module's mutable const state persists in the VM (retiring the #5296
        // `restore_module_globals` fakery for the covered subset).
        if !program.modules.is_empty() {
            return false;
        }
        // A session that holds a USER module is admitted to the (live-only) delta
        // path ONLY when every such module is a SIMPLE persistable user module
        // (LV5/LV5b): the relocatable-delta compile resolves `M.f()` / `M.const`
        // — and, since Issue #9723, `M.Sub.f()` / `M.SomeType` — against the
        // live VM's realized module via the carried module surface
        // (`ReplModuleMetadata`, keyed by qualified path). Package modules
        // (`using X` — heavy `__init__`-captured state), inner `using`/`import`,
        // and module-level macros/type-aliases are NOT persistable this way and
        // stay on the full recompile path (LV5b remainder). The
        // `InteractiveUtils` auto-import is stateless/pre-realized in the parked
        // VM and is skipped.
        if !self.session_modules_persistable() {
            return false;
        }
        // Aliases, macros, enums, and `using` still route to the full recompile
        // path. Brand-new Main abstract and primitive definitions are admitted
        // by `input_defines_only_new_types` below (Issues #9784/#11635).
        if !program.type_aliases.is_empty()
            || !program.macros.is_empty()
            || !program.enums.is_empty()
            || !program.usings.is_empty()
        {
            return false;
        }
        // `@enum` definitions live in main statements rather than
        // `Program.enums`; `input_defines_only_new_types` validates their
        // append-only type names and the persistent compiled prefix carries
        // their ordered metadata (Issues #9784/#11635).
        // Struct definitions (Issue #9199 LV4): an input that defines ONLY
        // brand-new, non-parametric, no-inner-constructor structs IS eligible —
        // `try_live_delta_run` compiles them and appends their type defs to the
        // live registries (`Vm::install_appended_types`) at aligned `type_id`s
        // (world-neutral; a coarse dispatch-cache retire). A parametric /
        // inner-constructor / redefined struct (or a name colliding with an
        // existing type/function) still takes the full recompile path.
        if !self.input_defines_only_new_types(program) {
            return false;
        }
        // Function definitions (Issues #9199/#9784): brand-new Main generics and
        // ordinary method extensions/replacements are eligible. For a mutation,
        // the compiler refreshes only the transitive Main caller slice and the
        // held VM advances worlds in source order. Base/preload-owned generics
        // remain on the full path.
        if !self.input_defines_only_new_generic_functions(program) {
            return false;
        }
        // Opaque runtime eval can define methods invisibly to the compiler.
        if repl_support::program_defines_via_opaque_eval(program) {
            return false;
        }
        true
    }

    /// Whether every USER module this session holds is a SIMPLE persistable module
    /// the LV5/LV5b live delta path can re-enter soundly (Issues #9199 / #9723).
    /// Fail-closed: returns `false` if the session loaded any package (a non-auto
    /// `using X`, whose module `__init__` establishes VM-local state that must
    /// re-run each eval) or holds a module with structure the live-append does
    /// not yet realize (inner `using`/`import`, module-level macros or
    /// type-aliases, a `baremodule`, or a mirror-untrackable binding — see
    /// `module_is_simple_persistable`). Those keep the full recompile path
    /// (LV5b remainder). Submodules and module-level struct/abstract/primitive
    /// types are admitted since Issue #9723. A module-free session (or one
    /// holding only the stateless `InteractiveUtils` auto-import) trivially
    /// qualifies. The default `InteractiveUtils` module is skipped: it is
    /// pre-realized in the parked live VM and carries no user mutable state.
    fn session_modules_persistable(&self) -> bool {
        if self
            .usings
            .iter()
            .any(|u| !self.auto_import_modules.contains(&u.module))
        {
            return false;
        }
        self.modules
            .iter()
            .all(|m| self.auto_import_modules.contains(&m.name) || module_is_simple_persistable(m))
    }

    /// Whether the session holds any NON-auto user module whose realized const
    /// state lives only in the parked live VM (Issue #9199 LV5). Such a session
    /// has no valid fresh-VM delta path — a fresh `Vm` never re-runs the module
    /// body, so the module's realized state would be missing — so its deltas must
    /// take the live path or fall back to the full recompile.
    fn has_stateful_user_module(&self) -> bool {
        self.modules
            .iter()
            .any(|m| !self.auto_import_modules.contains(&m.name))
    }

    /// LV2 live-append fast path (Issue #9199 — the crux). Compile the RAW `input`
    /// as a relocatable delta main (global slots seeded from the held live VM's
    /// frame-0), splice it onto that VM, and re-enter — WITHOUT rebuilding the VM,
    /// re-injecting globals, or re-running module bodies. Returns the reentered VM
    /// (ready for `run()`) plus the compile bundle to install and the main-scope
    /// name set for cross-eval extraction. Returns `None` (the caller falls back
    /// to the fresh path) whenever the delta is not cleanly appendable — an
    /// uninstalled function reference, a preload splice, a slot-seed mismatch,
    /// or a compile error (the fresh path then re-reports it).
    ///
    /// The caller guarantees `program` is appendable under
    /// `persistent_delta_eligible` (expressions, globals, brand-new generic
    /// functions, or brand-new simple concrete structs; no modules/usings or
    /// redefinitions) and that `self.live_vm` is aligned with
    /// `self.persistent_compile.bundle`. `eval_seed` re-seeds the VM's RNG to
    /// match the fresh path's per-eval determinism.
    fn try_live_delta_run(
        &mut self,
        program: &Program,
        eval_seed: u64,
    ) -> Option<PreparedLiveDelta> {
        let mut current_enum_defs = Vec::new();
        collect_main_enum_defs(&program.main, &mut current_enum_defs);
        // Read the live VM's frame-0 layout + code tail + function/struct counts
        // (the immutable borrow ends before the mutable `take` below).
        let (
            seed,
            live_code_len,
            live_fn_count,
            live_struct_count,
            live_abstract_count,
            live_primitive_count,
            live_enum_count,
        ) = {
            let vm = self.live_vm.as_ref()?;
            (
                vm.global_slot_names().to_vec(),
                vm.code_len(),
                vm.functions_len(),
                vm.struct_defs_len(),
                vm.abstract_types_len(),
                vm.primitive_types_len(),
                vm.enum_defs_len(),
            )
        };
        let live_global_slot_count = seed.len();
        let prev = self.persistent_compile.as_ref()?;
        // LV3 (Issue #9199): a compiled-definition live-append installs the new
        // function bodies at aligned live indices `[P..P+u]` — which holds only
        // while the live VM still has EXACTLY `P` functions (the compile prefix's
        // count). A successful prior LV3 append advances the snapshot with the
        // live VM; an opaque runtime `@eval` can still grow only the live count
        // past `P`. If this input defines functions and the counts drift, fall back to
        // the full recompile (it rebuilds a fresh VM whose function count
        // re-syncs with a refreshed prefix). Expression/global deltas (no new
        // functions) are unaffected: they reference only prefix functions
        // (`< P`), which stay aligned however far the live count has grown.
        if !program.functions.is_empty() && live_fn_count != prev.prefix_function_count() {
            return None;
        }
        // LV4 (Issue #9199): the type analogue. A compiled-struct live-append
        // installs each new concrete struct at its aligned `type_id` (== its
        // index in `struct_defs`), which holds only while the live VM still has
        // EXACTLY `S` struct defs (the compile prefix's count). A successful prior
        // LV4 append advances both counts; an out-of-band live-only mutation can
        // still desynchronize them. If this input defines structs and the live count has
        // drifted, fall back to the full recompile (it rebuilds a fresh VM whose
        // struct-def count re-syncs with a refreshed prefix). Expression / global
        // / function deltas that instantiate only PRIOR structs are unaffected:
        // their `NewStruct(tid)` reference `tid < S`, which stays aligned however
        // far the live count has grown.
        if !program.structs.is_empty() && live_struct_count != prev.prefix_struct_def_count() {
            return None;
        }
        if !program.abstract_types.is_empty()
            && live_abstract_count != prev.prefix_abstract_type_count()
        {
            return None;
        }
        if !program.primitive_types.is_empty()
            && live_primitive_count != prev.prefix_primitive_type_count()
        {
            return None;
        }
        if !current_enum_defs.is_empty() && live_enum_count != prev.prefix_enum_def_count() {
            return None;
        }
        // A compile error here means the fresh path will re-report it to the user;
        // `Ok(None)` means the delta is not cleanly appendable. Either way, fall
        // back by returning `None`.
        let appendable = repl_support::relocatable_delta_compile(
            prev,
            program,
            &self.global_types,
            &self.global_struct_names,
            &seed,
            live_code_len,
        )
        .ok()??;
        // Main-inline lambdas are lifted helpers rather than `program.functions`,
        // so the pre-compile definition check above cannot see them. Re-establish
        // the same positional invariant before any live mutation whenever the
        // structural appendability scan discovered helper bodies (Issue #11569).
        if !appendable.new_functions.is_empty() && live_fn_count != prev.prefix_function_count() {
            return None;
        }
        let appended_function_names = appendable
            .new_functions
            .iter()
            .map(|function| function.info.name.clone())
            .collect();
        let source_function_indices = appendable.source_function_indices.clone();
        let appended_struct_names = appendable
            .new_struct_defs
            .iter()
            .map(|definition| definition.name.clone())
            .collect();
        let appended_abstract_type_names = appendable
            .new_abstract_types
            .iter()
            .map(|definition| definition.name.clone())
            .collect();
        let appended_primitive_type_names = appendable
            .new_primitive_types
            .iter()
            .map(|definition| definition.name.clone())
            .collect();
        let appended_enum_names = appendable
            .new_enum_defs
            .iter()
            .map(|definition| definition.name.clone())
            .collect();
        let definition_activations = appendable.definition_activations.clone();
        let runtime_nominal_templates = appendable.runtime_nominal_templates.clone();
        // Validate every operation that used to be able to reject AFTER taking
        // and partially extending the live VM. The returned opaque setup owns
        // the specialization tail and fully built activation maps; after this
        // point the only fallible mutation is the all-or-nothing nominal
        // reservation below (Issue #9784).
        let append_setup = self.live_vm.as_ref()?.prepare_repl_append_setup(
            ReplAppendDefinitionCounts {
                function_bodies: appendable.new_functions.len(),
                source_functions: appendable.source_function_indices.len(),
                structs: appendable.new_struct_defs.len(),
                abstract_types: appendable.new_abstract_types.len(),
                primitive_types: appendable.new_primitive_types.len(),
                enums: appendable.new_enum_defs.len(),
            },
            appendable.new_specializable_functions,
            &definition_activations,
            &appendable.specializable_updates,
        )?;
        let can_reach_runtime_enum = runtime_nominal_templates
            .iter()
            .any(|template| matches!(template.definition, RuntimeNominalDefInfo::Enum(_)));
        let enum_registry_transaction = (!appendable.new_enum_defs.is_empty()
            || can_reach_runtime_enum)
            .then(EnumRegistryTransaction::begin);

        // Commit: take the live VM, grow frame-0 for the new globals, install the
        // new type defs (Issue #9199 LV4) and dormant function bodies (Issue
        // #9199 LV3), then splice the relocated main and re-enter. Source-ordered
        // `DefineEvalFunction` markers publish those bodies during `run()`
        // (Issues #9784/#11477). Done only after the compile proved
        // cleanly appendable, so `take` never leaves the session without a VM on a
        // fall-back path.
        let mut vm = self.live_vm.take()?;
        // Reserve types before any other live mutation. A leftover pending tail
        // proves the VM/compiler snapshots are not aligned; restore the VM and
        // take the conservative full-compile path without growing slots or code.
        if !vm.reserve_appended_nominal_types(
            appendable.new_struct_defs,
            appendable.new_abstract_types,
            appendable.new_primitive_types,
            appendable.new_enum_defs,
        ) {
            self.live_vm = Some(vm);
            return None;
        }
        vm.grow_global_slots(&appendable.new_globals);
        for func in &appendable.new_functions {
            // The compiler laid these out at aligned live indices; append them in
            // order BEFORE the main (their bodies precede it in the appended
            // region) and BEFORE `run()` so the main / a later eval can dispatch
            // to them.
            vm.install_appended_function_body(func.info.clone(), &func.body, &func.source);
        }
        vm.reenter_appended_main(
            &appendable.new_main,
            &appendable.new_source_map,
            StableRng::new(eval_seed),
        );
        vm.install_prepared_repl_append_setup(append_setup);
        // `reenter_appended_main` resets the transient per-run graphical flag, so
        // re-arm it AFTER the splice for a graphical host (Issue #9262).
        if self.graphical_display {
            vm.enable_graphical_display();
        }
        Some(PreparedLiveDelta {
            vm,
            main_scope_names: appendable.main_scope_names,
            next_persistent: appendable.next_persistent,
            first_appended_global_slot_index: live_global_slot_count,
            first_appended_function_index: live_fn_count,
            appended_function_names,
            source_function_indices,
            first_appended_struct_index: live_struct_count,
            appended_struct_names,
            first_appended_abstract_type_index: live_abstract_count,
            appended_abstract_type_names,
            first_appended_primitive_type_index: live_primitive_count,
            appended_primitive_type_names,
            first_appended_enum_index: live_enum_count,
            appended_enum_names,
            definition_activations,
            runtime_nominal_templates,
            enum_registry_transaction,
        })
    }

    /// Returns the number of replay-prefix statements this merge spliced at the
    /// front of `main`: prior `using`/`import` markers followed by prior-enum
    /// `Stmt::EnumDef` statements. `inject_globals` places persisted-global init
    /// statements after EXACTLY that prefix: imports must activate before a
    /// reconstructed global can refer to an imported binding, and replayed enum
    /// member stores must run before the carried globals that override them. The
    /// init statements still precede every statement the user typed this eval,
    /// including a current-input `@enum` (Issues #9701 and #11216).
    fn merge_definitions(&self, program: &mut Program) -> usize {
        // Collect the method SIGNATURES defined in this input. A prior method is
        // "being redefined" only when the same name AND parameter types reappear;
        // a same-name method with different parameter types is a NEW method that
        // must coexist, preserving multiple dispatch across evaluations
        // (Issue #9173).
        let new_func_sigs: std::collections::HashSet<String> = program
            .functions
            .iter()
            .map(|f| method_signature_key(f))
            .collect();

        // Add existing methods that aren't redefined by this input
        for func in &self.functions {
            if !new_func_sigs.contains(&method_signature_key(func)) {
                program.functions.push(std::sync::Arc::new(func.clone()));
            }
        }

        // Collect new struct names
        let new_struct_names: std::collections::HashSet<String> =
            program.structs.iter().map(|s| s.name.clone()).collect();

        // Add existing structs that aren't being redefined
        for s in &self.structs {
            if !new_struct_names.contains(&s.name) {
                program.structs.push(s.clone());
            }
        }

        // Re-fold prior abstract / primitive / enum type definitions, symmetric
        // with structs (Issue #9701): later same-name definitions in THIS input
        // win, everything else is carried forward so the type name stays bound
        // (`isa`, subtyping in later struct defs, and `::T` dispatch keep
        // working across evals).
        let new_abstract_names: std::collections::HashSet<String> = program
            .abstract_types
            .iter()
            .map(|a| a.name.clone())
            .collect();
        for a in &self.abstract_types {
            if !new_abstract_names.contains(&a.name) {
                program.abstract_types.push(a.clone());
            }
        }

        let new_primitive_names: std::collections::HashSet<String> = program
            .primitive_types
            .iter()
            .map(|p| p.name.clone())
            .collect();
        for p in &self.primitive_types {
            if !new_primitive_names.contains(&p.name) {
                program.primitive_types.push(p.clone());
            }
        }

        let new_type_alias_names: std::collections::HashSet<String> = program
            .type_aliases
            .iter()
            .map(|alias| alias.name.clone())
            .collect();
        for alias in &self.type_aliases {
            if !new_type_alias_names.contains(&alias.name) {
                program.type_aliases.push(alias.clone());
            }
        }

        // Re-inject prior `@enum` definitions not redefined by this input as
        // `Stmt::EnumDef` at the FRONT of `main` (Issue #9701). `@enum` lowers
        // to a main STATEMENT (`Program.enums` stays empty), so the re-fold
        // must go through `main`: the compiler's `collect_enum_types` walks the
        // main block to resolve the enum TYPE name (`c isa Color`), and
        // re-running the definition re-registers the runtime enum registry and
        // member globals (idempotent — same members, same values).
        let mut this_input_enums = Vec::new();
        collect_main_enum_defs(&program.main, &mut this_input_enums);
        let new_enum_names: std::collections::HashSet<String> =
            this_input_enums.iter().map(|e| e.name.clone()).collect();
        let enum_stmts: Vec<Stmt> = self
            .enums
            .iter()
            .filter(|e| !new_enum_names.contains(&e.name))
            .map(|e| Stmt::EnumDef {
                enum_def: e.clone(),
                published_members: self.enum_published_members.get(&e.name).cloned(),
                span: e.span,
            })
            .collect();
        let replayed_enum_stmts = enum_stmts.len();
        if !enum_stmts.is_empty() {
            program.main.stmts.splice(0..0, enum_stmts);
        }

        // Collect new module names
        let new_module_names: std::collections::HashSet<String> =
            program.modules.iter().map(|m| m.name.clone()).collect();

        // Add existing modules that aren't being redefined
        for m in &self.modules {
            if !new_module_names.contains(&m.name) {
                program.modules.push(m.clone());
            }
        }

        // Replay prior imports before the current input. Submodule aliases now
        // activate when each executable `using` marker runs, so carrying only
        // the metadata would lose an existing ambiguity on a full REPL rebuild.
        // Keep semantically distinct imports from the same module: `using A`
        // and later `using A: Sub` have different precedence (Issues
        // #11203/#11216).
        let current_usings = std::mem::take(&mut program.usings);
        let mut merged_usings = self.usings.clone();
        for using in current_usings {
            if !merged_usings
                .iter()
                .any(|prior| same_using_import(prior, &using))
            {
                merged_usings.push(using);
            }
        }
        let replay_markers: Vec<Stmt> = self
            .usings
            .iter()
            .map(|using| Stmt::Using {
                module: using.module.clone(),
                span: using.span,
            })
            .collect();
        let replayed_prefix_stmts = replay_markers.len() + replayed_enum_stmts;
        program.main.stmts.splice(0..0, replay_markers);
        program.usings = merged_usings;

        replayed_prefix_stmts
    }

    /// Inject global variable initializations at the start of the program.
    ///
    /// Returns the globals whose runtime `Value` could **not** be reconstructed as
    /// an init expression (so no init statement was emitted for them). The caller
    /// carries these across the eval by seeding the VM directly with the real
    /// `Value` (Issue #8260) — e.g. an OrdinaryDiffEq `ODEProblem`, whose
    /// `kwargs::Base.Pairs` field has no init-expr form, was otherwise silently
    /// dropped and the next eval raised `UndefVarError`.
    /// `replayed_prefix_stmts` is the number of prior-import markers and prior
    /// enum definitions `merge_definitions` spliced at the front of `main` this
    /// eval (0 on the delta path, which never merges). The init statements are
    /// inserted after exactly those: imports must activate before reconstructed
    /// globals are compiled, and replayed enum member stores must not clobber the
    /// carried globals. A CURRENT-INPUT `@enum` must still execute AFTER the
    /// injected globals so this eval's statements see prior globals first and
    /// run in source order (Issue #9701; codex review, PR #10248).
    fn inject_globals(
        &mut self,
        program: &mut Program,
        replayed_prefix_stmts: usize,
        struct_seed_rebindings: &std::collections::HashSet<String>,
        current_input_defines_struct: bool,
    ) -> Vec<(String, Value)> {
        let mut init_stmts = Vec::new();
        let mut seed_globals: Vec<(String, Value)> = Vec::new();
        let dummy_span = Span::new(0, 0, 0, 0, 0, 0);

        // Create assignment statements for each global variable
        for name in self.globals.variable_names() {
            if let Some(value) = self.globals.get(&name) {
                // A fresh full rebuild prepends reconstructed prior globals before
                // the current input's source-ordered type activation markers. If
                // this input repeats a struct-bearing workload, rebuilding one of
                // its globals through `TypeName(fields...)` would therefore try to
                // call the newly compiled constructor while that type is still
                // intentionally private. A binding that this input itself
                // reassigns does not need source reconstruction: carry its exact
                // pre-eval value into frame 0 instead. This also preserves the old
                // value for a self-rebinding RHS while avoiding any synthetic
                // constructor call before `DefineEvalStruct` (Issue #11546).
                if current_input_defines_struct && struct_seed_rebindings.contains(&name) {
                    seed_globals.push((name, value));
                    continue;
                }
                // Persistent model (Issue #9199 S3): value-carry the leak-prone
                // simple globals (scalars, strings, callables — the #9182 / #9157 /
                // #8976 family) DIRECTLY into the VM binding table instead of
                // rebuilding them as init statements, retiring the value→expr
                // round-trip for them. Struct-, array-, and other heap-backed
                // globals fall through to the source-reconstruction fallback below, because
                // value-carrying a heap array loses its element type and breaks
                // downstream dispatch (e.g. `det(A)` on a carried Symbolics matrix
                // reports "expected numeric array element, got Any"). Broadening the
                // persistent carrier to those is a later slice.
                if is_persistent_carriable(&value) {
                    seed_globals.push((name, value));
                    continue;
                }
                // Track whether any branch below emits an init statement for this
                // global. If none does, it cannot be reconstructed from source and
                // must be value-carried instead of silently dropped (Issue #8260).
                // `name` is moved by several branches, so snapshot it up front and
                // re-fetch the value only on the (rare) dropped path.
                let init_len_before = init_stmts.len();
                let seed_name = name.clone();

                // Large heap-backed globals (e.g. an `@animate` animation whose
                // cumulative frames hold O(frames^2) points) must NOT be rebuilt as an
                // AST init expression: `value_to_init_expr` /
                // `struct_instance_to_literal` would materialize a giant `Expr` tree,
                // transiently allocating multiple GB and OOM-aborting the iOS REPL
                // (Issue #9229). Value-carry them across
                // the eval instead; the struct heap is transplanted regardless, so a
                // carried `StructRef` stays valid. The estimate is O(budget), never
                // O(data), and the cap is far above any hand-written literal.
                let leaf_estimate = value_literal_leaf_estimate(
                    &value,
                    &self.last_struct_heap,
                    MAX_PERSISTED_GLOBAL_LITERAL_LEAVES,
                );
                if leaf_estimate >= MAX_PERSISTED_GLOBAL_LITERAL_LEAVES {
                    if let Some(carried) = self.globals.get(&seed_name) {
                        seed_globals.push((seed_name, carried));
                    }
                    continue;
                }

                if let Value::Memory(mem) = &value {
                    let mem = mem.borrow();
                    if let Some(mut stmts) = memory_value_to_init_stmts(&name, &mem, dummy_span) {
                        init_stmts.append(&mut stmts);
                        continue;
                    }
                }

                if let Some(expr) = value_to_init_expr(&value, &self.last_struct_heap, dummy_span) {
                    let stmt = Stmt::Assign {
                        var: name,
                        value: expr,
                        span: dummy_span,
                    };
                    init_stmts.push(stmt);
                    continue;
                }

                // Empty arrays (`ps = []`, `Int[]`, `Any[]`) have no init expr from
                // value_to_init_expr (it yields None so module initializers win,
                // Issue #5296), so re-create them explicitly here or the binding is
                // dropped and the next eval raises UndefVarError (Issue #7151).
                if let Some(expr) =
                    empty_array_init_expr(&value, &self.last_struct_heap, dummy_span)
                {
                    let stmt = Stmt::Assign {
                        var: name,
                        value: expr,
                        span: dummy_span,
                    };
                    init_stmts.push(stmt);
                    continue;
                }

                // Handle StructRef specially by converting it to a Struct literal.
                if let Value::StructRef(idx) = value {
                    if let Some(struct_instance) = self.last_struct_heap.get(idx) {
                        // Use struct_name directly (Rational is defined in Pure Julia, not in program.structs)
                        if let Some(literal) = struct_instance_to_literal(
                            struct_instance,
                            &struct_instance.struct_name,
                        ) {
                            let stmt = Stmt::Assign {
                                var: name,
                                value: Expr::Literal(literal, dummy_span),
                                span: dummy_span,
                            };
                            init_stmts.push(stmt);
                        }
                    }
                } else if let Some(arr) = native_array_value_ref(&value) {
                    // Handle Array with StructRefs - convert each StructRef to Literal::Struct
                    let arr_borrow = arr.borrow();
                    if let ArrayData::StructRefs(ref struct_refs) = arr_borrow.data {
                        let mut elements = Vec::new();
                        for &struct_ref_idx in struct_refs {
                            if let Some(struct_instance) = self.last_struct_heap.get(struct_ref_idx)
                            {
                                if let Some(literal) = struct_instance_to_literal(
                                    struct_instance,
                                    &struct_instance.struct_name,
                                ) {
                                    elements.push(Expr::Literal(literal, dummy_span));
                                } else {
                                    // If conversion fails, skip this array
                                    break;
                                }
                            } else {
                                // If struct_instance not found, skip this array
                                break;
                            }
                        }
                        // Only create assignment if all elements were converted successfully
                        if elements.len() == struct_refs.len() {
                            let stmt = Stmt::Assign {
                                var: name,
                                value: Expr::ArrayLiteral {
                                    elements,
                                    shape: arr_borrow.shape.clone(),
                                    span: dummy_span,
                                },
                                span: dummy_span,
                            };
                            init_stmts.push(stmt);
                        }
                    } else if let Some(literal) = value_to_literal(&value) {
                        // Handle other array types (F64, I64, Bool, etc.)
                        let stmt = Stmt::Assign {
                            var: name,
                            value: Expr::Literal(literal, dummy_span),
                            span: dummy_span,
                        };
                        init_stmts.push(stmt);
                    }
                } else if let Value::Memory(mem) = value {
                    let mem = mem.borrow();
                    if let Some(mut stmts) = memory_value_to_init_stmts(&name, &mem, dummy_span) {
                        init_stmts.append(&mut stmts);
                    }
                } else if let Value::NamedTuple(ref nt) = value {
                    // Handle NamedTuple - convert to NamedTupleLiteral
                    let mut fields = Vec::new();
                    let mut all_convertible = true;
                    for (field_name, field_value) in nt.names.iter().zip(nt.values.iter()) {
                        if let Some(field_literal) = value_to_literal(field_value) {
                            fields.push((
                                field_name.clone().into(),
                                Expr::Literal(field_literal, dummy_span),
                            ));
                        } else {
                            // If any field cannot be converted, skip this NamedTuple
                            all_convertible = false;
                            break;
                        }
                    }
                    if all_convertible {
                        let stmt = Stmt::Assign {
                            var: name,
                            value: Expr::NamedTupleLiteral {
                                fields,
                                span: dummy_span,
                            },
                            span: dummy_span,
                        };
                        init_stmts.push(stmt);
                    }
                } else if let Value::Range(ref r) = value {
                    // Ranges have no Literal form; reconstruct the
                    // `start:step:stop` expression so the binding survives into
                    // the next evaluation (Issue: `t = 0:0.01:2π` then using `t`
                    // raised UndefVarError because Range globals were dropped).
                    let lit = |x: f64| {
                        if r.is_float {
                            Literal::Float(x)
                        } else {
                            Literal::Int(x as i64)
                        }
                    };
                    let stmt = Stmt::Assign {
                        var: name,
                        value: Expr::Range {
                            start: Box::new(Expr::Literal(lit(r.start), dummy_span)),
                            step: Some(Box::new(Expr::Literal(lit(r.step), dummy_span))),
                            stop: Box::new(Expr::Literal(lit(r.stop), dummy_span)),
                            span: dummy_span,
                        },
                        span: dummy_span,
                    };
                    init_stmts.push(stmt);
                } else if let Some(literal) = value_to_literal(&value) {
                    let stmt = Stmt::Assign {
                        var: name,
                        value: Expr::Literal(literal, dummy_span),
                        span: dummy_span,
                    };
                    init_stmts.push(stmt);
                } else if let Some(expr) = callable_value_to_expr(&value, dummy_span) {
                    // Handle Function and ComposedFunction
                    let stmt = Stmt::Assign {
                        var: name,
                        value: expr,
                        span: dummy_span,
                    };
                    init_stmts.push(stmt);
                }

                // No branch produced an init statement: the value has no source
                // representation (e.g. a struct carrying a `Base.Pairs` field). Carry
                // the real runtime Value across the eval instead of dropping it so the
                // next eval still sees the binding (Issue #8260).
                if init_stmts.len() == init_len_before {
                    if let Some(carried) = self.globals.get(&seed_name) {
                        seed_globals.push((seed_name, carried));
                    }
                }
            }
        }

        // Prepend initialization statements to main — but AFTER any leading
        // replay prefix (the prior imports and enum definitions that
        // `merge_definitions` splices to the front; Issues #9701/#11216). An
        // `EnumDef` compiles to `RegisterEnum` +
        // a `StoreAny(member)` per member, so if the replay ran AFTER these
        // inits it would clobber the carried member globals with the original
        // enum values; running the inits after the replay lets the persisted
        // values (the source of truth for current bindings) win. The offset is
        // EXACTLY the replayed statement count — never a scan for leading
        // `EnumDef`s, which would also skip past a CURRENT-INPUT `@enum` and
        // let the injected globals execute in the middle of this eval's
        // statements (`@enum Color red green; c = green` with a carried
        // `green` global then bound the stale carried value to `c`; codex
        // review, PR #10248). A current-input `@enum` member store thus runs
        // after the inits, in source order, and wins within its own eval.
        if !init_stmts.is_empty() {
            debug_assert!(
                program
                    .main
                    .stmts
                    .iter()
                    .take(replayed_prefix_stmts)
                    .all(|s| matches!(s, Stmt::Using { .. } | Stmt::EnumDef { .. })),
                "the first {replayed_prefix_stmts} main statements must be the \
                 replayed import and enum definitions spliced by merge_definitions"
            );
            program
                .main
                .stmts
                .splice(replayed_prefix_stmts..replayed_prefix_stmts, init_stmts);
        }

        seed_globals
    }

    /// Project runtime-authoritative global values into the transitional mirror.
    /// The caller supplies only prior globals and value-binding stores that
    /// actually executed; no Program/AST assignment scan participates (#9784).
    fn extract_globals_from_vm<R: crate::rng::RngLike>(
        &mut self,
        vm: &Vm<R>,
        authoritative_globals: &std::collections::HashSet<String>,
    ) {
        for var_name in authoritative_globals {
            if let Some(value) = vm.get_global(var_name) {
                // Handle StructRef and retain its compiler-facing struct metadata.
                if let Value::StructRef(idx) = value {
                    if let Ok(arr) = vm_repl::linalg_value_to_array_value(
                        Value::StructRef(idx),
                        vm.get_struct_heap(),
                        "repl_global",
                        None,
                    ) {
                        self.global_types.insert(
                            var_name.clone(),
                            ValueType::ArrayOf(arr.element_type(), None),
                        );
                        self.global_struct_names.remove(var_name);
                        self.globals.set(var_name, value);
                        continue;
                    }

                    // Store the StructRef index in globals and retain its type metadata.
                    if let Some(struct_instance) = vm.get_struct_heap().get(idx) {
                        // Store type information for type inference
                        // Save struct_name to resolve type_id from struct_table during compilation
                        self.global_struct_names
                            .insert(var_name.clone(), struct_instance.struct_name.to_string());
                        // Use type_id from struct_instance as placeholder (will be resolved during compilation)
                        self.global_types
                            .insert(var_name.clone(), ValueType::Struct(struct_instance.type_id));
                    }
                    // Also save StructRef index to globals
                    self.globals.set(var_name, value);
                } else {
                    // Infer type from value
                    let value_type = if let Some(arr_ref) = native_array_value_ref(&value) {
                        // Preserve element type for proper type inference
                        let arr = arr_ref.borrow();
                        ValueType::ArrayOf(arr.element_type(), None)
                    } else {
                        match &value {
                            Value::I64(_) => ValueType::I64,
                            Value::F64(_) => ValueType::F64,
                            Value::Str(_) => ValueType::Str,
                            Value::Memory(mem) => {
                                ValueType::MemoryOf(mem.borrow().element_type().clone())
                            }
                            Value::NamedTuple(_) => ValueType::Tuple, // NamedTuple is a Tuple in type system
                            _ => ValueType::Any,
                        }
                    };
                    self.global_types.insert(var_name.clone(), value_type);
                    self.globals.set(var_name, value);
                }
            }
        }
    }

    /// Keep the compiler type metadata for the `ans` global in sync with the
    /// value just stored into it. `ans` is assigned outside
    /// `extract_globals_from_vm` (which is what maintains `global_types` /
    /// `global_struct_names` for ordinary globals), so without this its hint would
    /// retain whatever the previous `ans` value inferred. Under the persistent
    /// model, where `ans` is value-carried into the next VM with no init statement,
    /// a stale hint mis-compiles the next `ans` load (Issue #9199). Mirrors the
    /// type inference in `extract_globals_from_vm`.
    fn record_ans_type(&mut self, value: &Value) {
        if let Value::StructRef(idx) = value {
            if let Some(si) = self.last_struct_heap.get(*idx) {
                self.global_struct_names
                    .insert("ans".to_string(), si.struct_name.to_string());
                self.global_types
                    .insert("ans".to_string(), ValueType::Struct(si.type_id));
                return;
            }
        }
        // Non-struct: drop any stale struct name and record a scalar/array type.
        self.global_struct_names.remove("ans");
        let value_type = if let Some(arr_ref) = native_array_value_ref(value) {
            ValueType::ArrayOf(arr_ref.borrow().element_type(), None)
        } else {
            match value {
                Value::I64(_) => ValueType::I64,
                Value::F64(_) => ValueType::F64,
                Value::Str(_) => ValueType::Str,
                Value::Memory(mem) => ValueType::MemoryOf(mem.borrow().element_type().clone()),
                Value::NamedTuple(_) => ValueType::Tuple,
                _ => ValueType::Any,
            }
        };
        self.global_types.insert("ans".to_string(), value_type);
    }

    /// Capture module-level mutable constants from the VM so they persist into
    /// the next evaluation. Walks every module (and submodule) in the program,
    /// reads each top-level `const`/global by its qualified global name, and
    /// stores the current value keyed by that qualified name (Issue #5296).
    fn extract_module_globals_from_vm<R: crate::rng::RngLike>(
        &mut self,
        vm: &Vm<R>,
        program: &Program,
    ) {
        let mut qualified_names = Vec::new();
        for module in &program.modules {
            collect_module_constant_paths(module, "", &mut qualified_names);
        }
        // Issue #9199 LV5: a live-delta eval carries NO modules in `program`
        // (it only references them), but its module const state was mutated in
        // the re-entered live VM under the session's known modules. Refresh those
        // too so `module_globals` stays coherent — otherwise a later full-recompile
        // fallback would `restore_module_globals` to a STALE value and lose the
        // live mutations. `self.modules` holds only PRIOR modules at this point
        // (`store_definitions` runs later), so a module-DEFINITION eval still
        // relies on the `program.modules` pass above; the two together keep the
        // mirror current on both paths.
        for module in &self.modules {
            collect_module_constant_paths(module, "", &mut qualified_names);
        }
        for qualified in qualified_names {
            // Block-wrapped `const` declarations are not registered as module
            // constants by the compiler, so the VM stores them under the bare name
            // (`_CURRENT_SERIES`) rather than the qualified one. Try the qualified
            // global first, then fall back to the bare name (Issue #5296).
            let value = vm.get_global(&qualified).or_else(|| {
                let bare = qualified.rsplit('.').next().unwrap_or(&qualified);
                vm.get_global(bare)
            });
            if let Some(value) = value {
                self.module_globals.insert(qualified, value);
            }
        }

        let runtime_import_binding_names = repl_support::runtime_import_binding_names(program);
        self.module_runtime_global_names.extend(
            vm.repl_explicit_global_write_names()
                .iter()
                .filter(|name| {
                    !is_main_binding_name(name)
                        && !repl_support::is_runtime_import_metadata_binding(name)
                        && !runtime_import_binding_names.contains_key(*name)
                })
                .cloned(),
        );
        let runtime_names: Vec<String> = self.module_runtime_global_names.iter().cloned().collect();
        for qualified in runtime_names {
            if let Some(value) = vm.get_global(&qualified) {
                self.module_globals.insert(qualified, value);
            } else {
                self.module_globals.remove(&qualified);
                self.module_runtime_global_names.remove(&qualified);
            }
        }
    }

    /// Restore persisted module-level state by rewriting the matching `const`
    /// initializers in the re-injected module bodies. The module body runs before
    /// `main` on every eval and would otherwise reset the binding; replacing the
    /// initializer expression makes the module re-initialize to the persisted
    /// value instead. Values that cannot be reconstructed are left untouched, so
    /// the module's original initializer still runs (Issue #5296).
    ///
    /// `newly_defined` holds the qualified paths of every module the CURRENT
    /// input (re)defines. A module tree rooted in that set is skipped: upstream
    /// `julia` REPLACES the module binding on redefinition, so the new module
    /// must start from its OWN initializers (state reset), not resume the old
    /// module's persisted state (Issue #10232). The filter is READ-ONLY on
    /// `self.module_globals` — an errored (re)definition eval therefore leaves
    /// the prior module's persisted state intact; on a successful run
    /// `extract_module_globals_from_vm` re-mirrors the fresh state. A submodule
    /// is always (re)defined together with its top-level parent, so skipping by
    /// the top-level module name covers the whole tree.
    fn restore_module_globals(
        &self,
        program: &mut Program,
        newly_defined: &std::collections::HashSet<String>,
    ) {
        if self.module_globals.is_empty() {
            return;
        }
        let heap = &self.last_struct_heap;
        for module in &mut program.modules {
            if newly_defined.contains(&module.name) {
                continue;
            }
            restore_module_constants(module, "", &self.module_globals, heap);
        }
    }

    /// Empty the `__init__` body of every already-realized user module that the
    /// current input did NOT (re)define, so a module's `__init__` runs ONCE per
    /// realization instead of on every accumulate-and-recompile pass (Issue #9199
    /// S4). This is part of the sole production persistent model.
    ///
    /// `newly_defined` holds the paths of modules realized by the CURRENT input
    /// (so their `__init__` must run this eval); `self.initialized_module_paths`
    /// holds paths realized on a PRIOR eval. A module in the latter but not the
    /// former is a re-merged prior module whose `__init__` would otherwise re-fire.
    /// The `__init__` function object is kept (so its module surface / name
    /// resolution is unchanged) but its body is cleared, so the compiled call is a
    /// no-op. Module body const bindings are untouched and still persist via
    /// `restore_module_globals`, so only the init side effects are suppressed.
    fn suppress_module_reinit(
        &self,
        program: &mut Program,
        newly_defined: &std::collections::HashSet<String>,
    ) {
        if self.initialized_module_paths.is_empty() {
            return;
        }
        for module in &mut program.modules {
            empty_reinitialized_module_init(
                module,
                "",
                &self.initialized_module_paths,
                newly_defined,
            );
        }
    }

    fn invalidate_type_aliases_for_value_rebindings(
        &mut self,
        value_rebindings: &std::collections::HashSet<String>,
    ) {
        if value_rebindings.is_empty() {
            return;
        }
        self.type_aliases
            .retain(|alias| !value_rebindings.contains(&alias.name));
        self.type_alias_index.clear();
        for (index, alias) in self.type_aliases.iter().enumerate() {
            self.type_alias_index.insert(alias.name.clone(), index);
        }
    }

    /// Store new function, struct, and module definitions.
    fn store_definitions(
        &mut self,
        program: &Program,
        committed_value_rebindings: &std::collections::HashSet<String>,
        function_limit: usize,
        current_input_program_function_count: Option<usize>,
        struct_limit: usize,
        abstract_type_limit: usize,
        primitive_type_limit: usize,
        enum_limit: usize,
        store_modules: bool,
    ) {
        // Update methods, keyed by signature (name + parameter types). Replacing
        // only the exact-signature match means redefining a method updates it in
        // place, while a new method of the same name is appended — so a generic
        // function accumulates its methods across evaluations (Issue #9173).
        let mut source_functions = program
            .functions
            .iter()
            .take(current_input_program_function_count.unwrap_or(program.functions.len()))
            .filter(|function| !repl_support::is_markerless_lowered_function(function))
            .map(|function| function.as_ref().clone())
            .chain(repl_support::collect_main_inline_named_functions(program))
            .collect::<Vec<_>>();
        source_functions.sort_by_key(|function| {
            (
                function.span.definition_order,
                function.span.start,
                function.span.end,
            )
        });
        let mut seen_source_methods = std::collections::HashSet::new();
        source_functions
            .retain(|function| seen_source_methods.insert(method_signature_key(function)));
        for func in source_functions.iter().take(function_limit) {
            let key = method_signature_key(func);
            if let Some(&idx) = self.function_index.get(&key) {
                self.functions[idx] = func.clone();
            } else {
                let idx = self.functions.len();
                self.functions.push(func.clone());
                self.function_index.insert(key, idx);
            }
        }
        // Lowering helpers do not consume the Julia-visible source-method
        // prefix, but a later module/using/redefinition input may rebuild the
        // VM from accumulated IR. Retain current-input helpers from both the
        // hoisted function vector and inline main so value-carried closures
        // remain callable after that rebuild (Issue #9784).
        let current_input_program_function_count =
            current_input_program_function_count.unwrap_or(program.functions.len());
        for func in program
            .functions
            .iter()
            .take(current_input_program_function_count)
            .filter(|function| repl_support::is_markerless_lowered_function(function))
            .map(|function| function.as_ref().clone())
            .chain(repl_support::collect_main_inline_anonymous_functions(
                program,
            ))
        {
            let key = method_signature_key(&func);
            if let Some(&idx) = self.function_index.get(&key) {
                self.functions[idx] = func;
            } else {
                let idx = self.functions.len();
                self.functions.push(func);
                self.function_index.insert(key, idx);
            }
        }

        // Update macros (replace existing by name, add new). Carried into the
        // lowering of later evaluations so a macro defined here is usable by a
        // later expression on the same session (Issue #9172).
        for macro_def in &program.macros {
            if let Some(&idx) = self.macro_index.get(&macro_def.name) {
                self.macros[idx] = macro_def.clone();
            } else {
                let idx = self.macros.len();
                self.macros.push(macro_def.clone());
                self.macro_index.insert(macro_def.name.clone(), idx);
            }
        }

        // Update structs
        for s in program.structs.iter().take(struct_limit) {
            if let Some(&idx) = self.struct_index.get(&s.name) {
                self.structs[idx] = s.clone();
            } else {
                let idx = self.structs.len();
                self.structs.push(s.clone());
                self.struct_index.insert(s.name.clone(), idx);
            }
        }

        // Update abstract / primitive / enum type definitions (replace existing
        // by name, add new), symmetric with structs (Issue #9701) — so the type
        // name survives to later evals and `merge_definitions` can re-fold it.
        for a in program.abstract_types.iter().take(abstract_type_limit) {
            if let Some(&idx) = self.abstract_type_index.get(&a.name) {
                self.abstract_types[idx] = a.clone();
            } else {
                let idx = self.abstract_types.len();
                self.abstract_types.push(a.clone());
                self.abstract_type_index.insert(a.name.clone(), idx);
            }
        }

        for p in program.primitive_types.iter().take(primitive_type_limit) {
            if let Some(&idx) = self.primitive_type_index.get(&p.name) {
                self.primitive_types[idx] = p.clone();
            } else {
                let idx = self.primitive_types.len();
                self.primitive_types.push(p.clone());
                self.primitive_type_index.insert(p.name.clone(), idx);
            }
        }

        // A non-alias value assignment replaces the Julia binding and removes
        // any persisted static alias of the same name. Rebuild the compact
        // name index after retaining the unaffected aliases.
        self.invalidate_type_aliases_for_value_rebindings(committed_value_rebindings);

        // Type aliases are definition-time type bindings just like the
        // abstract/primitive definitions above. Retain their canonical target
        // and source order across REPL evaluations (Issue #11086).
        for alias in &program.type_aliases {
            if committed_value_rebindings.contains(&alias.name) {
                continue;
            }
            if let Some(&idx) = self.type_alias_index.get(&alias.name) {
                self.type_aliases[idx] = alias.clone();
            } else {
                let idx = self.type_aliases.len();
                self.type_aliases.push(alias.clone());
                self.type_alias_index.insert(alias.name.clone(), idx);
            }
        }

        // `@enum` lowers to a `Stmt::EnumDef` inside `main` (`Program.enums`
        // stays empty), so collect the definitions from the statements. The
        // program was already merged, so this also re-stores the re-injected
        // prior enums — an idempotent same-name replace.
        let mut main_enum_defs = Vec::new();
        collect_main_enum_defs_with_publication(&program.main, &mut main_enum_defs);
        for (e, published_members) in main_enum_defs.iter().take(enum_limit) {
            if let Some(&idx) = self.enum_index.get(&e.name) {
                self.enums[idx] = e.clone();
            } else {
                let idx = self.enums.len();
                self.enums.push(e.clone());
                self.enum_index.insert(e.name.clone(), idx);
            }
            if let Some(published_members) = published_members {
                self.enum_published_members
                    .insert(e.name.clone(), published_members.clone());
            } else {
                self.enum_published_members.remove(&e.name);
            }
        }

        // Update modules (replace existing, add new)
        if store_modules {
            for m in &program.modules {
                if let Some(&idx) = self.module_index.get(&m.name) {
                    self.modules[idx] = m.clone();
                } else {
                    let idx = self.modules.len();
                    self.modules.push(m.clone());
                    self.module_index.insert(m.name.clone(), idx);
                }
            }
        }

        // Update usings without collapsing selective/non-selective provenance.
        for using in &program.usings {
            if !self.usings.iter().any(|u| same_using_import(u, using)) {
                self.usings.push(using.clone());
            }
        }
    }

    /// Retain exactly the import statements whose execution marker completed
    /// before a catchable toplevel error. The VM trace is validated before this
    /// method is called, so indexing cannot silently skip malformed evidence.
    fn store_reached_usings(&mut self, usings: &[UsingImport], reached_indices: &[usize]) {
        for index in reached_indices {
            let using = &usings[*index];
            if !self
                .usings
                .iter()
                .any(|stored| same_using_import(stored, using))
            {
                self.usings.push(using.clone());
            }
        }
    }

    /// Persist only runtime-conditional nominal definitions that the VM
    /// actually committed. Static reconstruction of the enclosing control flow
    /// would revive skipped declarations, so the observed activation payload is
    /// the sole source of truth (Issue #11654).
    fn store_runtime_nominal_activations(&mut self, activations: &[RuntimeNominalActivation]) {
        for activation in activations {
            let qualified_name = match &activation.definition {
                RuntimeNominalDefInfo::Struct(definition) => definition.source.name.as_str(),
                RuntimeNominalDefInfo::AbstractType(definition) => definition.name.as_str(),
                RuntimeNominalDefInfo::PrimitiveType(definition) => definition.name.as_str(),
                RuntimeNominalDefInfo::Enum(definition) => definition.name.as_str(),
            };
            // The owning module is already retained and replayed as a module.
            // Mirroring its qualified child in Main's flat nominal collections
            // publishes the same type twice on the next full rebuild (#11686).
            if qualified_name.contains('.') {
                continue;
            }
            self.runtime_nominal_names
                .insert(qualified_name.to_string());
            match &activation.definition {
                RuntimeNominalDefInfo::Struct(definition) => {
                    let source = definition.source.as_ref().clone();
                    if let Some(&index) = self.struct_index.get(&source.name) {
                        self.structs[index] = source;
                    } else {
                        let index = self.structs.len();
                        self.struct_index.insert(source.name.clone(), index);
                        self.structs.push(source);
                    }
                }
                RuntimeNominalDefInfo::AbstractType(definition) => {
                    let source = AbstractTypeDef {
                        name: definition.name.clone(),
                        parent: definition.parent.clone(),
                        type_params: definition.type_params.clone(),
                        span: activation.span,
                    };
                    if let Some(&index) = self.abstract_type_index.get(&source.name) {
                        self.abstract_types[index] = source;
                    } else {
                        let index = self.abstract_types.len();
                        self.abstract_type_index.insert(source.name.clone(), index);
                        self.abstract_types.push(source);
                    }
                }
                RuntimeNominalDefInfo::PrimitiveType(definition) => {
                    let source = PrimitiveTypeDef {
                        name: definition.name.clone(),
                        parent: definition.parent.clone(),
                        bits: definition.bits,
                        span: activation.span,
                    };
                    if let Some(&index) = self.primitive_type_index.get(&source.name) {
                        self.primitive_types[index] = source;
                    } else {
                        let index = self.primitive_types.len();
                        self.primitive_type_index.insert(source.name.clone(), index);
                        self.primitive_types.push(source);
                    }
                }
                RuntimeNominalDefInfo::Enum(definition) => {
                    let source = EnumDef {
                        name: definition.name.clone(),
                        base_type: definition.base_type.clone(),
                        members: definition
                            .members
                            .iter()
                            .map(|(name, value)| crate::ir::core::EnumMember {
                                name: name.clone(),
                                value: *value,
                                span: activation.span,
                            })
                            .collect(),
                        span: activation.span,
                    };
                    if let Some(&index) = self.enum_index.get(&source.name) {
                        self.enums[index] = source;
                    } else {
                        let index = self.enums.len();
                        self.enum_index.insert(source.name.clone(), index);
                        self.enums.push(source);
                    }
                    if let Some(published_members) = &activation.published_members {
                        self.enum_published_members
                            .insert(definition.name.clone(), published_members.clone());
                    }
                }
            }
        }
    }

    /// Replace only modules that began executing before an error with inert,
    /// reached-definition shells. Existing modules from earlier evaluations are
    /// left untouched; source-later modules and failed-body expressions are not
    /// persisted (Issue #11721).
    fn store_recovered_modules(
        &mut self,
        source_modules: &[Module],
        replay: &RecoveredModuleReplay,
    ) {
        for source in source_modules {
            let root_path = source.name.as_str();
            if !replay.module_paths.contains(root_path) {
                continue;
            }
            let recovered = recover_module_after_error(
                source,
                "",
                replay,
                &self.module_globals,
                &self.last_struct_heap,
            );
            if let Some(&index) = self.module_index.get(&recovered.name) {
                self.modules[index] = recovered;
            } else {
                let index = self.modules.len();
                self.module_index.insert(recovered.name.clone(), index);
                self.modules.push(recovered);
            }
        }
    }

    /// Reset the session, clearing all variables and definitions.
    pub fn reset(&mut self) {
        #[cfg(debug_assertions)]
        debug_assert_eq!(
            std::thread::current().id(),
            self.owner_thread,
            "REPLSession::reset called from wrong thread (Issue #8675)",
        );
        self.globals.clear();
        self.functions.clear();
        self.function_index.clear();
        self.macros.clear();
        self.macro_index.clear();
        self.structs.clear();
        self.struct_index.clear();
        self.abstract_types.clear();
        self.abstract_type_index.clear();
        self.primitive_types.clear();
        self.primitive_type_index.clear();
        self.type_aliases.clear();
        self.type_alias_index.clear();
        self.enums.clear();
        self.enum_index.clear();
        self.enum_published_members.clear();
        self.runtime_nominal_names.clear();
        self.modules.clear();
        self.module_index.clear();
        self.usings.clear();
        self.ans = None;
        self.eval_count = 0;
        self.last_struct_heap.clear();
        self.module_globals.clear();
        self.module_runtime_global_names.clear();
        // A reset session has realized no modules; the next `module M …end`
        // re-runs its `__init__` (Issue #9199 S4).
        self.initialized_module_paths.clear();
        // Compiler-facing type hints for previously seen globals (Issue #9193):
        // left uncleared, a name shadowing a Base generic (e.g. `first = true`)
        // would outlive reset() and get misresolved as a stale global-variable
        // reference in later evals instead of the builtin function, even though
        // `globals` above was just cleared out from under it.
        self.global_types.clear();
        self.global_struct_names.clear();
        // Drop the accumulated input-delta compile cache: a reset session has no
        // prior definitions, so the next Persistent eval rebuilds from scratch
        // (Issue #9199 S5). This is part of "reset == fresh session".
        self.persistent_compile = None;
        // Drop the live VM held across evals: a reset session must observe none
        // of the prior globals / struct heap / dispatch caches / world the live
        // VM carries, so the next Persistent eval starts from a fresh VM (Issue
        // #9199 LV1). Part of "reset == fresh session".
        self.live_vm = None;
        self.persisted_callable_snapshot = None;
    }

    /// Get the last VM's struct heap (for resolving StructRefs in display)
    pub fn get_struct_heap(&self) -> &[StructInstance] {
        &self.last_struct_heap
    }

    /// Get the last evaluation result (ans).
    pub fn get_ans(&self) -> Option<&Value> {
        self.ans.as_ref()
    }

    /// Get all variable names in the session.
    pub fn variable_names(&self) -> Vec<String> {
        self.globals.variable_names()
    }

    /// Get all user-visible function names defined in the session.
    pub fn function_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .functions
            .iter()
            .filter(|func| !is_internal_lowered_function(func))
            .map(|func| func.name.clone())
            .collect();
        names.sort();
        names.dedup();
        names
    }

    /// Get non-relative module names imported with `using` in this session.
    pub fn imported_module_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .usings
            .iter()
            .filter(|using| !using.is_relative)
            .map(|using| using.module.clone())
            .collect();
        names.sort();
        names.dedup();
        names
    }

    /// Get field names for session globals backed by known struct definitions.
    pub fn field_names_by_object(&self) -> Vec<(String, Vec<String>)> {
        let mut fields_by_object = Vec::new();
        for name in self.globals.variable_names() {
            let Some(struct_name) = self.global_struct_name_for_value(&name) else {
                continue;
            };
            let Some(def) = self.structs.iter().find(|def| {
                normalized_struct_base_name(&def.name) == normalized_struct_base_name(&struct_name)
            }) else {
                continue;
            };
            fields_by_object.push((
                name,
                def.fields.iter().map(|field| field.name.clone()).collect(),
            ));
        }
        fields_by_object.sort_by(|a, b| a.0.cmp(&b.0));
        fields_by_object
    }

    fn global_struct_name_for_value(&self, name: &str) -> Option<String> {
        if let Some(struct_name) = self.global_struct_names.get(name) {
            return Some(struct_name.clone());
        }
        match self.globals.get(name)? {
            Value::Struct(s) => Some(s.struct_name.to_string()),
            Value::StructRef(idx) => self
                .last_struct_heap
                .get(idx)
                .map(|instance| instance.struct_name.to_string()),
            _ => None,
        }
    }

    /// Split input into top-level expressions.
    /// Returns a vector of (start_byte, end_byte, source_text) for each expression.
    /// If parsing fails, returns None.
    /// Uses simple heuristic splitting based on newlines (Julia REPL style).
    pub fn split_expressions(&self, input: &str) -> Option<Vec<(usize, usize, String)>> {
        // Split on newlines when outside of block structures
        // This matches Julia REPL behavior: each top-level line is evaluated separately
        let mut exprs = Vec::new();
        let mut current_start = 0;
        let mut in_block = 0i32;
        let mut in_string = false;
        let mut in_triple_string = false;
        let mut escape_next = false;
        let mut in_line_comment = false;
        let mut block_comment_depth = 0i32;
        let mut paren_depth = 0i32;
        let mut bracket_depth = 0i32;

        let chars: Vec<char> = input.chars().collect();
        let mut i = 0;

        while i < chars.len() {
            let ch = chars[i];

            // Handle escape sequences in strings
            if escape_next {
                escape_next = false;
                i += 1;
                continue;
            }

            // End of line resets line comment state (but not block comment)
            if ch == '\n' {
                in_line_comment = false;
            }

            // Block comment handling (#= ... =#) - supports nesting
            if !in_string && !in_triple_string && !in_line_comment {
                // Check for block comment start (#=)
                if ch == '#' && i + 1 < chars.len() && chars[i + 1] == '=' {
                    // If we're at the start of an expression (nothing meaningful before this),
                    // we'll need to update current_start when the comment ends
                    block_comment_depth += 1;
                    i += 2;
                    continue;
                }
                // Check for block comment end (=#)
                if block_comment_depth > 0
                    && ch == '='
                    && i + 1 < chars.len()
                    && chars[i + 1] == '#'
                {
                    block_comment_depth -= 1;
                    i += 2;
                    // When we exit the outermost block comment, skip past it for expression extraction
                    if block_comment_depth == 0 {
                        // Check if everything from current_start to here is just whitespace/comments
                        let prefix: String = chars[current_start..i].iter().collect();
                        let prefix_is_just_whitespace_and_comments = prefix.trim().is_empty()
                            || prefix.trim_start().starts_with("#=")
                            || prefix.lines().all(|line| {
                                let t = line.trim();
                                t.is_empty() || t.starts_with('#') || t == "=#"
                            });
                        if prefix_is_just_whitespace_and_comments {
                            current_start = i;
                        }
                    }
                    continue;
                }
            }

            // Skip everything inside block comments
            if block_comment_depth > 0 {
                i += 1;
                continue;
            }

            // Line comment handling (skip # to end of line)
            // Only if not starting a block comment (#=)
            if !in_string && !in_triple_string && ch == '#' {
                // Already checked for #= above, so this is a line comment
                in_line_comment = true;
                i += 1;
                continue;
            }

            if in_line_comment {
                i += 1;
                continue;
            }

            // Escape sequence in strings
            if (in_string || in_triple_string) && ch == '\\' {
                escape_next = true;
                i += 1;
                continue;
            }

            // Triple-quoted string handling (""")
            if !in_string
                && i + 2 < chars.len()
                && ch == '"'
                && chars[i + 1] == '"'
                && chars[i + 2] == '"'
            {
                if in_triple_string {
                    in_triple_string = false;
                    i += 3;
                    continue;
                } else {
                    in_triple_string = true;
                    i += 3;
                    continue;
                }
            }

            // Regular string handling
            if !in_triple_string && ch == '"' {
                in_string = !in_string;
                i += 1;
                continue;
            }

            // Skip processing inside strings
            if in_string || in_triple_string {
                i += 1;
                continue;
            }

            // Track parentheses and brackets (for multi-line expressions)
            if ch == '(' {
                paren_depth += 1;
            } else if ch == ')' {
                paren_depth -= 1;
            } else if ch == '[' {
                bracket_depth += 1;
            } else if ch == ']' {
                bracket_depth -= 1;
            }

            // Track block depth - check keywords at word boundaries
            let is_keyword = |kw: &str| -> bool {
                let kw_bytes = kw.as_bytes();
                let kw_len = kw_bytes.len();
                if i + kw_len > chars.len() {
                    return false;
                }
                // Compare chars directly without String allocation
                for (j, &b) in kw_bytes.iter().enumerate() {
                    if chars[i + j] != b as char {
                        return false;
                    }
                }
                // Check not preceded by alphanumeric
                if i > 0 && (chars[i - 1].is_alphanumeric() || chars[i - 1] == '_') {
                    return false;
                }
                // Check not followed by alphanumeric
                if i + kw_len < chars.len()
                    && (chars[i + kw_len].is_alphanumeric() || chars[i + kw_len] == '_')
                {
                    return false;
                }
                true
            };

            if is_keyword("function")
                || is_keyword("if")
                || is_keyword("for")
                || is_keyword("while")
                || is_keyword("begin")
                || is_keyword("try")
                || is_keyword("module")
                || is_keyword("struct")
                || is_keyword("let")
                || is_keyword("quote")
                || is_keyword("macro")
                || is_keyword("do")
            {
                in_block += 1;
            } else if is_keyword("end") {
                in_block = (in_block - 1).max(0);
            }

            // Check for expression boundary at newline
            // Split when: outside blocks, balanced parens/brackets, at newline
            if ch == '\n' && in_block == 0 && paren_depth == 0 && bracket_depth == 0 {
                // Look ahead to see if there's more content (non-empty, non-comment line)
                let mut j = i + 1;
                // Skip blank lines
                while j < chars.len() && chars[j] == '\n' {
                    j += 1;
                }
                // Skip leading whitespace on the next line
                while j < chars.len() && (chars[j] == ' ' || chars[j] == '\t') {
                    j += 1;
                }

                // If there's more content
                if j < chars.len() && chars[j] != '\n' {
                    // Extract the current expression (up to and including this newline)
                    let end_pos = i + 1;
                    let text: String = chars[current_start..end_pos].iter().collect();
                    let trimmed = text.trim();

                    // Check if this is a non-comment expression
                    // Filter out lines that are just comments
                    let is_just_comment = trimmed.lines().all(|line| {
                        let line_trimmed = line.trim();
                        line_trimmed.is_empty() || line_trimmed.starts_with('#')
                    });

                    if !trimmed.is_empty() && !is_just_comment {
                        // Extract only the non-comment content
                        let filtered: String = trimmed
                            .lines()
                            .filter(|line| !line.trim().is_empty() && !line.trim().starts_with('#'))
                            .collect::<Vec<_>>()
                            .join("\n");
                        if !filtered.is_empty() {
                            exprs.push((current_start, end_pos, filtered));
                        }
                    }

                    // Always advance current_start past processed content
                    current_start = end_pos;
                    // Skip the blank lines we already processed
                    while current_start < j
                        && (chars[current_start] == '\n'
                            || chars[current_start] == ' '
                            || chars[current_start] == '\t')
                    {
                        current_start += 1;
                    }
                }
            }

            i += 1;
        }

        // Add remaining content
        if current_start < chars.len() {
            let text: String = chars[current_start..].iter().collect();
            let trimmed = text.trim();

            // Check if this is a non-comment expression
            let is_just_comment = trimmed.lines().all(|line| {
                let line_trimmed = line.trim();
                line_trimmed.is_empty() || line_trimmed.starts_with('#')
            });

            if !trimmed.is_empty() && !is_just_comment {
                // Extract only the non-comment content
                let filtered: String = trimmed
                    .lines()
                    .filter(|line| !line.trim().is_empty() && !line.trim().starts_with('#'))
                    .collect::<Vec<_>>()
                    .join("\n");
                if !filtered.is_empty() {
                    exprs.push((current_start, chars.len(), filtered));
                }
            }
        }

        // Only return if there are multiple expressions
        if exprs.len() > 1 {
            Some(exprs)
        } else {
            None
        }
    }

    /// Check if input contains multiple top-level expressions.
    pub fn has_multiple_expressions(&self, input: &str) -> bool {
        self.split_expressions(input).is_some()
    }
}

fn is_import_only_input(program: &Program) -> bool {
    !program.usings.is_empty()
        && program.main.stmts.is_empty()
        && program.abstract_types.is_empty()
        && program.primitive_types.is_empty()
        && program.type_aliases.is_empty()
        && program.structs.is_empty()
        && program.functions.is_empty()
        && program.modules.is_empty()
        && program.macros.is_empty()
        && program.enums.is_empty()
}

fn same_using_import(left: &UsingImport, right: &UsingImport) -> bool {
    left.module == right.module
        && left.symbols == right.symbols
        && left.is_import == right.is_import
        && left.is_relative == right.is_relative
        && left.relative_level == right.relative_level
        && left.alias_bindings == right.alias_bindings
}

fn validate_reached_using_indices(
    activations: &[(String, usize)],
    using_count: usize,
) -> Option<Vec<usize>> {
    let mut previous = None;
    let mut validated = Vec::new();
    for (owner_module, index) in activations {
        if !owner_module.is_empty() {
            continue;
        }
        let index = *index;
        if index >= using_count || previous.is_some_and(|previous| index <= previous) {
            return None;
        }
        previous = Some(index);
        validated.push(index);
    }
    Some(validated)
}

#[cfg(test)]
mod reached_using_validation_tests_11748 {
    use super::validate_reached_using_indices;

    #[test]
    fn selects_only_main_owned_source_order() {
        let activations = vec![
            (String::new(), 0),
            ("Nested11748".to_string(), 0),
            (String::new(), 2),
        ];
        assert_eq!(
            validate_reached_using_indices(&activations, 3),
            Some(vec![0, 2])
        );
    }

    #[test]
    fn rejects_duplicate_or_out_of_range_main_identity() {
        assert_eq!(
            validate_reached_using_indices(&[(String::new(), 1), (String::new(), 1)], 2),
            None
        );
        assert_eq!(
            validate_reached_using_indices(&[(String::new(), 2)], 2),
            None
        );
    }
}

fn seed_prior_type_aliases(type_aliases: &[TypeAliasDef], modules: &[Module]) {
    for alias in type_aliases {
        crate::lowering::type_alias::register(
            &alias.name,
            alias.params.clone(),
            &alias.target_type,
        );
    }
    for module in modules {
        seed_prior_module_type_aliases(module, "");
    }
}

fn seed_prior_module_type_aliases(module: &Module, prefix: &str) {
    let module_path = if prefix.is_empty() {
        module.name.clone()
    } else {
        format!("{prefix}.{}", module.name)
    };
    for alias in &module.type_aliases {
        crate::lowering::type_alias::register_qualified_only(
            &format!("{module_path}.{}", alias.name),
            alias.params.clone(),
            &alias.target_type,
        );
    }
    for submodule in &module.submodules {
        seed_prior_module_type_aliases(submodule, &module_path);
    }
}

fn is_internal_lowered_function(function: &Function) -> bool {
    repl_support::is_markerless_lowered_function(function)
}

/// Whether a global's runtime value can be carried DIRECTLY into the next eval's
/// VM binding table with the same Julia-visible value as source reconstruction
/// (Issue #9199 S3).
///
/// Carriable: simple scalar/string/callable values — exactly the leak-prone
/// globals the #9182/#9157/#8976 fix burst chased through the value→expr rebuild.
/// They carry faithfully and are the whole point of retiring the round-trip.
///
/// NOT carriable (routed through source reconstruction instead): struct-, array-,
/// tuple-, and other heap-backed values. Value-carrying a heap array loses its
/// element type (`det(A)` on a carried Symbolics matrix then reports "expected
/// numeric array element, got Any"), and a container may nest such a value, so
/// this predicate is deliberately conservative — anything not provably simple
/// falls back to the correct, slower reconstruction path. Broadening this to
/// structs/arrays is a later slice of the epic.
fn is_persistent_carriable(value: &Value) -> bool {
    // A native-array carrier is heap-backed — never carry it here.
    if is_native_array_value(value) {
        return false;
    }
    matches!(
        value,
        Value::I8(_)
            | Value::I16(_)
            | Value::I32(_)
            | Value::I64(_)
            | Value::I128(_)
            | Value::U8(_)
            | Value::U16(_)
            | Value::U32(_)
            | Value::U64(_)
            | Value::U128(_)
            | Value::F16(_)
            | Value::F32(_)
            | Value::F64(_)
            | Value::Bool(_)
            | Value::Char(_)
            | Value::Str(_)
            | Value::Function(_)
            | Value::Closure(_)
            | Value::ComposedFunction(_)
    )
}

/// A method-identity key for REPL accumulation: the function name plus its
/// positional parameter types (and a varargs marker). Two definitions with the
/// same key are the same method (a redefinition that should replace in place);
/// different keys are distinct methods of the same generic function that must
/// both be retained so multiple dispatch survives across evaluations
/// (Issue #9173).
fn method_signature_key(func: &Function) -> String {
    use std::fmt::Write;
    let mut key = String::new();
    key.push(if repl_support::is_markerless_lowered_function(func) {
        'H'
    } else {
        'S'
    });
    key.push(':');
    let _ = write!(key, "{}(", func.name);
    for (i, param) in func.params.iter().enumerate() {
        if i > 0 {
            key.push(',');
        }
        match &param.type_annotation {
            Some(ty) => {
                let _ = write!(key, "{}", ty);
            }
            None => key.push_str("Any"),
        }
        if param.is_varargs {
            key.push_str("...");
        }
    }
    key.push(')');
    key
}

/// Collect every `@enum` definition statement in `block`, recursing into plain
/// `begin ... end` blocks — mirroring the compiler's `collect_enum_types`
/// (Issue #9701). `@enum` lowers to a `Stmt::EnumDef` inside `main` (the
/// `Program.enums` field stays empty), so REPL definition storage and re-merge
/// must walk the statements.
fn collect_main_enum_defs(block: &Block, out: &mut Vec<EnumDef>) {
    for stmt in &block.stmts {
        match stmt {
            Stmt::EnumDef { enum_def, .. } => out.push(enum_def.clone()),
            Stmt::Block(inner) => collect_main_enum_defs(inner, out),
            _ => {}
        }
    }
}

fn collect_main_enum_defs_with_publication(
    block: &Block,
    out: &mut Vec<(EnumDef, Option<Vec<String>>)>,
) {
    for stmt in &block.stmts {
        match stmt {
            Stmt::EnumDef {
                enum_def,
                published_members,
                ..
            } => out.push((enum_def.clone(), published_members.clone())),
            Stmt::Block(inner) => collect_main_enum_defs_with_publication(inner, out),
            _ => {}
        }
    }
}

fn record_observed_enum_member_publication(
    block: &mut Block,
    reached_enum_names: &std::collections::HashSet<String>,
    vm: &Vm<StableRng>,
) {
    for stmt in &mut block.stmts {
        match stmt {
            Stmt::EnumDef {
                enum_def,
                published_members,
                ..
            } if reached_enum_names.contains(&enum_def.name) => {
                let observed = enum_def
                    .members
                    .iter()
                    .filter(|member| {
                        matches!(
                            vm.get_global(&member.name),
                            Some(Value::Enum { type_name, value })
                                if type_name.as_str() == enum_def.name && value == member.value
                        )
                    })
                    .map(|member| member.name.clone())
                    .collect();
                *published_members = Some(observed);
            }
            Stmt::Block(inner) => {
                record_observed_enum_member_publication(inner, reached_enum_names, vm)
            }
            _ => {}
        }
    }
}

fn normalized_struct_base_name(name: &str) -> &str {
    let without_params = name.split('{').next().unwrap_or(name);
    without_params.rsplit('.').next().unwrap_or(without_params)
}

/// Build the qualified global name for a module path component, e.g.
/// (`""`, `"Plots"`) -> `"Plots"`, (`"A"`, `"B"`) -> `"A.B"`.
fn module_path_of(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_string()
    } else {
        format!("{prefix}.{name}")
    }
}

fn local_module_binding_name(qualified: &str) -> String {
    qualified
        .rsplit('.')
        .next()
        .unwrap_or(qualified)
        .to_string()
}

/// Build a replay-safe module whose executable body contains only observed
/// binding values. Nominal/function declarations are filtered by the VM's
/// reached activation prefix, and runtime-conditional nominals are converted
/// to ordinary declarations owned by this module. No condition, throw, or
/// arbitrary side-effect expression from the failed body is re-executed.
fn recover_module_after_error(
    source: &Module,
    prefix: &str,
    replay: &RecoveredModuleReplay,
    module_globals: &HashMap<String, Value>,
    heap: &[StructInstance],
) -> Module {
    let path = module_path_of(prefix, &source.name);
    let mut recovered = source.clone();

    recovered.functions.retain(|function| {
        let mut qualified = function.clone();
        if !qualified.name.contains('.') {
            qualified.name = format!("{path}.{}", qualified.name);
        }
        replay.function_names.contains(&qualified.name)
    });
    recovered.structs.retain(|definition| {
        replay
            .struct_names
            .contains(&format!("{path}.{}", definition.name))
    });
    recovered.abstract_types.retain(|definition| {
        replay
            .abstract_type_names
            .contains(&format!("{path}.{}", definition.name))
    });
    recovered.primitive_types.retain(|definition| {
        replay
            .primitive_type_names
            .contains(&format!("{path}.{}", definition.name))
    });
    // Alias/import/macro execution has no activation checkpoint yet. Retaining
    // whole-program metadata would publish source-later bindings, so fail closed
    // until those families gain the same reached-prefix protocol.
    recovered.type_aliases.clear();
    recovered.usings.clear();
    recovered.macros.clear();

    recovered.submodules = source
        .submodules
        .iter()
        .filter(|module| {
            replay
                .module_paths
                .contains(&module_path_of(&path, &module.name))
        })
        .map(|module| recover_module_after_error(module, &path, replay, module_globals, heap))
        .collect();

    let mut body_stmts =
        recovered_module_binding_stmts(&source.body.stmts, &path, replay, module_globals, heap);
    for activation in &replay.runtime_nominals {
        let qualified_name = match &activation.definition {
            RuntimeNominalDefInfo::Struct(definition) => definition.source.name.as_str(),
            RuntimeNominalDefInfo::AbstractType(definition) => definition.name.as_str(),
            RuntimeNominalDefInfo::PrimitiveType(definition) => definition.name.as_str(),
            RuntimeNominalDefInfo::Enum(definition) => definition.name.as_str(),
        };
        if qualified_name.rsplit_once('.').map(|(owner, _)| owner) != Some(path.as_str()) {
            continue;
        }
        match &activation.definition {
            RuntimeNominalDefInfo::Struct(definition) => {
                let mut source = definition.source.as_ref().clone();
                source.name = local_module_binding_name(&source.name);
                if !recovered
                    .structs
                    .iter()
                    .any(|existing| existing.name == source.name)
                {
                    recovered.structs.push(source);
                }
            }
            RuntimeNominalDefInfo::AbstractType(definition) => {
                let source = AbstractTypeDef {
                    name: local_module_binding_name(&definition.name),
                    parent: definition.parent.clone(),
                    type_params: definition.type_params.clone(),
                    span: activation.span,
                };
                if !recovered
                    .abstract_types
                    .iter()
                    .any(|existing| existing.name == source.name)
                {
                    recovered.abstract_types.push(source);
                }
            }
            RuntimeNominalDefInfo::PrimitiveType(definition) => {
                let source = PrimitiveTypeDef {
                    name: local_module_binding_name(&definition.name),
                    parent: definition.parent.clone(),
                    bits: definition.bits,
                    span: activation.span,
                };
                if !recovered
                    .primitive_types
                    .iter()
                    .any(|existing| existing.name == source.name)
                {
                    recovered.primitive_types.push(source);
                }
            }
            RuntimeNominalDefInfo::Enum(definition) => {
                let enum_def = EnumDef {
                    name: local_module_binding_name(&definition.name),
                    base_type: definition.base_type.clone(),
                    members: definition
                        .members
                        .iter()
                        .map(|(name, value)| crate::ir::core::EnumMember {
                            name: local_module_binding_name(name),
                            value: *value,
                            span: activation.span,
                        })
                        .collect(),
                    span: activation.span,
                };
                body_stmts.push(Stmt::EnumDef {
                    enum_def,
                    published_members: activation.published_members.clone(),
                    span: activation.span,
                });
            }
        }
    }
    recovered.body = Block {
        stmts: body_stmts,
        span: source.body.span,
    };
    recovered
}

fn recovered_module_binding_stmts(
    stmts: &[Stmt],
    module_path: &str,
    replay: &RecoveredModuleReplay,
    persisted: &HashMap<String, Value>,
    heap: &[StructInstance],
) -> Vec<Stmt> {
    let mut recovered = Vec::new();
    for stmt in stmts {
        match stmt {
            Stmt::Assign { var, span, .. } => {
                let qualified = format!("{module_path}.{var}");
                if let Some(value) = persisted
                    .get(&qualified)
                    .and_then(|value| value_to_module_init_expr(value, heap, *span, module_path))
                {
                    recovered.push(Stmt::Assign {
                        var: var.clone(),
                        value,
                        span: *span,
                    });
                }
            }
            Stmt::DestructuringAssign { targets, span, .. } => {
                for target in targets {
                    let qualified = format!("{module_path}.{target}");
                    if let Some(value) = persisted.get(&qualified).and_then(|value| {
                        value_to_module_init_expr(value, heap, *span, module_path)
                    }) {
                        recovered.push(Stmt::Assign {
                            var: target.clone(),
                            value,
                            span: *span,
                        });
                    }
                }
            }
            Stmt::Block(block) => recovered.extend(recovered_module_binding_stmts(
                &block.stmts,
                module_path,
                replay,
                persisted,
                heap,
            )),
            Stmt::If {
                then_branch,
                else_branch,
                ..
            } => {
                recovered.extend(recovered_module_binding_stmts(
                    &then_branch.stmts,
                    module_path,
                    replay,
                    persisted,
                    heap,
                ));
                if let Some(else_branch) = else_branch {
                    recovered.extend(recovered_module_binding_stmts(
                        &else_branch.stmts,
                        module_path,
                        replay,
                        persisted,
                        heap,
                    ));
                }
            }
            Stmt::EnumDef { enum_def, .. }
                if replay
                    .enum_names
                    .contains(&format!("{module_path}.{}", enum_def.name)) =>
            {
                recovered.push(stmt.clone());
            }
            _ => {}
        }
    }
    recovered
}

/// Collect the qualified path (`M`, `M.Sub`) of every module and submodule in
/// `modules`, matching the `Parent.Child` naming used for module-scoped globals
/// (Issue #9199 S4). Used to identify which modules the current input realizes.
/// Whether `module` is a user module the LV5/LV5b live delta path can realize
/// once soundly (Issues #9199 / #9723): top-level functions and const/global
/// bindings (plus an optional `__init__` and `export`s), module-level type
/// definitions, and recursively-simple submodules.
///
/// Admitted since Issue #9723 (LV5b):
/// - **Submodules** — the carried surface (`ReplModuleMetadata` via
///   `collect_module_info`) is keyed by qualified path (`M.Sub`), and the
///   state mirror (`collect_module_constant_paths` →
///   `extract_module_globals_from_vm` / `restore_module_constants`) recurses
///   the same qualified paths, so `M.Sub.f()` / `M.Sub.const` resolve on the
///   live path and stay mirror-coherent — provided every submodule is ITSELF
///   simple-persistable (checked recursively, incl. per-submodule
///   `module_bindings_fully_mirrorable`).
/// - **Module-level struct / abstract / primitive type definitions** — the
///   reused compile prefix already holds the qualified type defs (`M.Pt` in
///   `struct_defs` / `abstract_types` / `primitive_types`, registered at
///   full-recompile time), and the carried surface resolves `M.SomeType`
///   (type names are part of the `module_functions` name set).
///
/// Anything the carried surface or the delta's lowering does not cover stays
/// FAIL-CLOSED on the full recompile path (LV5b remainder): inner
/// `using`/`import` (re-exported surface unresolvable + package `__init__`
/// re-run semantics), module-level macros (a delta's lowering needs the macro
/// BODY, which the name-set surface does not carry), type aliases (the alias
/// TARGET mapping is not carried), and `baremodule`. A reference the live
/// delta then cannot resolve only falls back to the full recompile
/// (fail-safe), never miscompiles.
fn module_is_simple_persistable(module: &Module) -> bool {
    !module.is_bare
        && module.usings.is_empty()
        && module.macros.is_empty()
        && module.type_aliases.is_empty()
        && module_bindings_fully_mirrorable(module)
        && module.submodules.iter().all(module_is_simple_persistable)
}

/// Fail-closed guard (Issue #9199 LV5): every module-body binding the RESOLUTION
/// collector sees MUST also be tracked by the STATE-MIRROR collector, or the
/// module is NOT persistable on the live path.
///
/// Two collectors walk a module body and they are asymmetric:
/// - `collect_module_body_binding_names` (the RESOLUTION collector, via
///   `collect_module_info` → `ReplModuleMetadata`) recurses into `if`/`begin`
///   branches, `AssignExpr`, and empty `LetBlock` (Issue #7917: `module M; if
///   true; const x = 1; end; end` defines `M.x`). A binding it sees is
///   RESOLVABLE, so a delta referencing it can take the LIVE path.
/// - `collect_assign_vars_in_stmts` (the STATE-MIRROR collector, used by the
///   live-refresh loop AND `restore_module_constants`) walks `Assign`, `Block`,
///   and module-top-level `If` branches (Issue #9729). A binding it MISSES is
///   never mirrored into `module_globals`.
///
/// If the mirror misses a binding the resolution collector sees (for example an
/// `AssignExpr` or an empty `LetBlock`), a live-mutated module global would be
/// silently LOST when a later full-recompile fallback (`restore_module_globals`)
/// restores from the stale mirror and re-runs the body from its original
/// initializer, diverging from upstream and from the live VM's state. Such a
/// module is rejected here and routes to the full recompile path (LV5b): the session
/// full-recompiles every eval, which is coherent by construction. The invariant
/// is `mirror ⊇ resolution`. Reusing the SAME two functions the runtime uses
/// keeps this gate automatically in sync if either walker's coverage changes.
fn module_bindings_fully_mirrorable(module: &Module) -> bool {
    let mut resolution = std::collections::HashSet::new();
    repl_support::collect_module_body_binding_names(&module.body, &mut resolution);
    // The mirror emits `{prefix}.{var}`; with an empty prefix that is `.{var}`,
    // so strip the leading '.' to compare bare names.
    let mut mirror_qualified = Vec::new();
    collect_assign_vars_in_stmts(&module.body.stmts, "", &mut mirror_qualified);
    let mirror: std::collections::HashSet<&str> = mirror_qualified
        .iter()
        .map(|q| q.trim_start_matches('.'))
        .collect();
    resolution.iter().all(|name| mirror.contains(name.as_str()))
}

fn collect_module_paths(
    modules: &[Module],
    prefix: &str,
    out: &mut std::collections::HashSet<String>,
) {
    for module in modules {
        let path = module_path_of(prefix, &module.name);
        collect_module_paths(&module.submodules, &path, out);
        out.insert(path);
    }
}

fn collect_module_recovery_source_positions(
    modules: &[Module],
) -> (
    HashMap<String, usize>,
    HashMap<String, usize>,
    HashMap<String, usize>,
) {
    fn collect_body(
        stmts: &[Stmt],
        module_path: &str,
        functions: &mut HashMap<String, usize>,
        bindings: &mut HashMap<String, usize>,
    ) {
        for stmt in stmts {
            match stmt {
                Stmt::Assign { var, span, .. } => {
                    bindings.insert(format!("{module_path}.{var}"), span.start);
                }
                Stmt::DestructuringAssign { targets, span, .. } => {
                    for target in targets {
                        bindings.insert(format!("{module_path}.{target}"), span.start);
                    }
                }
                Stmt::FunctionDef { func, .. } | Stmt::EvalFunctionDef { func, .. } => {
                    let qualified = if func.name.contains('.') {
                        func.name.clone()
                    } else {
                        format!("{module_path}.{}", func.name)
                    };
                    functions.entry(qualified).or_insert(func.span.start);
                }
                Stmt::Block(block) => {
                    collect_body(&block.stmts, module_path, functions, bindings);
                }
                Stmt::If {
                    then_branch,
                    else_branch,
                    ..
                } => {
                    collect_body(&then_branch.stmts, module_path, functions, bindings);
                    if let Some(else_branch) = else_branch {
                        collect_body(&else_branch.stmts, module_path, functions, bindings);
                    }
                }
                _ => {}
            }
        }
    }

    fn collect(
        modules: &[Module],
        prefix: &str,
        module_positions: &mut HashMap<String, usize>,
        functions: &mut HashMap<String, usize>,
        bindings: &mut HashMap<String, usize>,
    ) {
        for module in modules {
            let path = module_path_of(prefix, &module.name);
            module_positions.insert(path.clone(), module.span.start);
            for function in &module.functions {
                let qualified = if function.name.contains('.') {
                    function.name.clone()
                } else {
                    format!("{path}.{}", function.name)
                };
                functions.entry(qualified).or_insert(function.span.start);
            }
            collect_body(&module.body.stmts, &path, functions, bindings);
            collect(
                &module.submodules,
                &path,
                module_positions,
                functions,
                bindings,
            );
        }
    }

    let mut module_positions = HashMap::new();
    let mut functions = HashMap::new();
    let mut bindings = HashMap::new();
    collect(
        modules,
        "",
        &mut module_positions,
        &mut functions,
        &mut bindings,
    );
    (module_positions, functions, bindings)
}

/// Recursively empty the `__init__` body of `module` (and its submodules) when the
/// module has already been realized on a prior eval and is not being (re)defined
/// by the current input, so its `__init__` runs once per realization (Issue #9199
/// S4). Each module is judged by its own qualified path, so a re-run submodule of
/// a re-defined parent is handled correctly.
fn empty_reinitialized_module_init(
    module: &mut Module,
    prefix: &str,
    initialized: &std::collections::HashSet<String>,
    newly_defined: &std::collections::HashSet<String>,
) {
    let path = module_path_of(prefix, &module.name);
    if initialized.contains(&path) && !newly_defined.contains(&path) {
        for func in &mut module.functions {
            if func.name == "__init__" {
                func.body.stmts.clear();
            }
        }
    }
    for submodule in &mut module.submodules {
        empty_reinitialized_module_init(submodule, &path, initialized, newly_defined);
    }
}

/// Collect the qualified names of every top-level module constant (and submodule
/// constant), matching the `Module.const` naming the compiler uses for module-
/// scoped globals (Issue #5296).
fn collect_module_constant_paths(module: &Module, prefix: &str, out: &mut Vec<String>) {
    let module_path = module_path_of(prefix, &module.name);
    collect_assign_vars_in_stmts(&module.body.stmts, &module_path, out);
    for submodule in &module.submodules {
        collect_module_constant_paths(submodule, &module_path, out);
    }
}

/// Collect top-level assignment targets in a module body. `const X = ...` lowers
/// to a `Stmt::Block` wrapping a `#__sjulia_declare_const__` marker and the actual
/// `Stmt::Assign`, so we descend one level into such blocks (Issue #5296). At
/// module top level, `if` branches do not introduce local scope, so assignments
/// inside them are module bindings and must be mirrored with the same traversal
/// `restore_assign_vars_in_stmts` uses (Issues #9199/#9729/#9989).
fn collect_assign_vars_in_stmts(stmts: &[Stmt], module_path: &str, out: &mut Vec<String>) {
    for stmt in stmts {
        match stmt {
            Stmt::Assign { var, .. } => out.push(format!("{module_path}.{var}")),
            Stmt::DestructuringAssign { targets, .. } => out.extend(
                targets
                    .iter()
                    .map(|target| format!("{module_path}.{target}")),
            ),
            Stmt::Block(block) => collect_assign_vars_in_stmts(&block.stmts, module_path, out),
            Stmt::If {
                then_branch,
                else_branch,
                ..
            } => {
                collect_assign_vars_in_stmts(&then_branch.stmts, module_path, out);
                if let Some(else_branch) = else_branch {
                    collect_assign_vars_in_stmts(&else_branch.stmts, module_path, out);
                }
            }
            _ => {}
        }
    }
}

/// Rewrite the initializer of each top-level module constant whose qualified name
/// has a persisted value, so the re-run module body re-initializes to that value
/// instead of its literal default (Issue #5296).
fn restore_module_constants(
    module: &mut Module,
    prefix: &str,
    persisted: &HashMap<String, Value>,
    heap: &[StructInstance],
) {
    let module_path = module_path_of(prefix, &module.name);
    restore_assign_vars_in_stmts(&mut module.body.stmts, &module_path, persisted, heap);
    for submodule in &mut module.submodules {
        restore_module_constants(submodule, &module_path, persisted, heap);
    }
}

/// Rewrite top-level module-constant initializers in `stmts`, descending into the
/// `Stmt::Block` and top-level `if` branches that can contain module bindings
/// (mirrors `collect_assign_vars_in_stmts`).
fn restore_assign_vars_in_stmts(
    stmts: &mut [Stmt],
    module_path: &str,
    persisted: &HashMap<String, Value>,
    heap: &[StructInstance],
) {
    for stmt in stmts {
        match stmt {
            Stmt::Assign { var, value, span } => {
                let qualified = format!("{module_path}.{var}");
                if let Some(persisted_value) = persisted.get(&qualified) {
                    if let Some(expr) =
                        value_to_module_init_expr(persisted_value, heap, *span, module_path)
                    {
                        *value = expr;
                    }
                }
            }
            Stmt::DestructuringAssign { targets, span, .. } => {
                let replacements = targets
                    .iter()
                    .map(|target| {
                        let qualified = format!("{module_path}.{target}");
                        let persisted_value = persisted.get(&qualified)?;
                        let value =
                            value_to_module_init_expr(persisted_value, heap, *span, module_path)?;
                        Some(Stmt::Assign {
                            var: target.clone(),
                            value,
                            span: *span,
                        })
                    })
                    .collect::<Option<Vec<_>>>();
                if let Some(replacements) = replacements {
                    *stmt = Stmt::Block(Block {
                        stmts: replacements,
                        span: *span,
                    });
                }
            }
            Stmt::Block(block) => {
                restore_assign_vars_in_stmts(&mut block.stmts, module_path, persisted, heap)
            }
            Stmt::If {
                then_branch,
                else_branch,
                ..
            } => {
                restore_assign_vars_in_stmts(&mut then_branch.stmts, module_path, persisted, heap);
                if let Some(else_branch) = else_branch {
                    restore_assign_vars_in_stmts(
                        &mut else_branch.stmts,
                        module_path,
                        persisted,
                        heap,
                    );
                }
            }
            _ => {}
        }
    }
}

fn memory_value_to_init_stmts(name: &str, mem: &MemoryValue, span: Span) -> Option<Vec<Stmt>> {
    let len = i64::try_from(mem.len()).ok()?;
    let mut stmts = Vec::with_capacity(mem.len().saturating_add(1));
    let constructor = Expr::Call {
        function: format!("Memory{{{}}}", mem.element_type().julia_type_name()).into(),
        args: vec![
            Expr::Var("undef".to_string().into(), span),
            Expr::Literal(Literal::Int(len), span),
        ],
        kwargs: Vec::new(),
        splat_mask: vec![false, false],
        kwargs_splat_mask: Vec::new(),
        span,
    };
    stmts.push(Stmt::Assign {
        var: name.to_string(),
        value: constructor,
        span,
    });

    for idx in 1..=mem.len() {
        let value = mem.get(idx).ok()?;
        let literal = value_to_literal(&value)?;
        let idx_i64 = i64::try_from(idx).ok()?;
        stmts.push(Stmt::Expr {
            expr: Expr::Call {
                function: "setindex!".to_string().into(),
                args: vec![
                    Expr::Var(name.to_string().into(), span),
                    Expr::Literal(literal, span),
                    Expr::Literal(Literal::Int(idx_i64), span),
                ],
                kwargs: Vec::new(),
                splat_mask: vec![false, false, false],
                kwargs_splat_mask: Vec::new(),
                span,
            },
            span,
        });
    }

    Some(stmts)
}

#[cfg(test)]
mod issue_9784_tests {
    include!("../../tests/internal/repl_session_9784_tests.rs");
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod lv1_live_vm_tests {
    //! LV1 (Issue #9199) live-VM machinery tests: the `Vm` append + re-enter
    //! primitive and the `REPLSession` live-VM hold/reset foundation. These
    //! exercise `Vm::reenter_appended_main` directly on a real, Base-compiled VM
    //! held by a persistent session — proving the runtime crux (splice a
    //! slot-free `main`, re-enter, PRESERVE frame-0 globals) independently of the
    //! delta-compiler contract that would feed it a relocatable `main` (deferred;
    //! see `docs/vm/ADR_REPL_EVAL_MODEL.md` §"Live-VM slice decomposition").
    use super::*;
    use subset_julia_vm_bytecode::Instr;

    fn with_large_stack<F: FnOnce() + Send + 'static>(f: F) {
        std::thread::Builder::new()
            .stack_size(64 * 1024 * 1024)
            .spawn(f)
            .unwrap()
            .join()
            .unwrap();
    }

    fn persistent() -> REPLSession {
        REPLSession::new(0)
    }

    fn i64_of(r: &REPLResult) -> Option<i64> {
        match &r.value {
            Some(Value::I64(v)) => Some(*v),
            _ => None,
        }
    }

    /// The crux: append a hand-built slot-free `main` (read the live global `x`
    /// by name, add 10) onto the VM a persistent session parked, re-enter it, and
    /// confirm it (a) runs from the appended entry, (b) reads the LIVE frame-0
    /// global, and (c) leaves that global — and the whole frame-0 — intact.
    #[test]
    fn reenter_appended_main_runs_and_preserves_frame0() {
        with_large_stack(|| {
            let mut session = REPLSession::new(0);
            let r = session.eval("x = 5");
            assert!(r.success, "setup eval failed: {:?}", r.error);
            assert!(session.has_live_vm(), "a persistent eval parks its VM");

            let mut vm = session.live_vm.take().expect("live VM held");
            assert!(matches!(vm.get_global("x"), Some(Value::I64(5))));
            let code_len_before = vm.code_len();

            // A slot-free delta `main`: `LoadGlobalAny` reads a global by NAME
            // (no frame-slot index), so it resolves against the live VM's frame-0
            // regardless of slot layout — exactly the alignment-safe shape LV1
            // targets.
            let main = vec![
                Instr::LoadGlobalAny("x".to_string()),
                Instr::PushI64(10),
                Instr::AddI64,
                Instr::ReturnAny,
            ];
            let source_map = vec![None; main.len()];
            vm.reenter_appended_main(&main, &source_map, StableRng::new(1));

            assert!(vm.code_len() > code_len_before, "the main was appended");
            let result = vm.run().expect("appended main runs");
            assert!(
                matches!(result, Value::I64(15)),
                "read live x (5) + 10, got {result:?}"
            );
            assert!(
                matches!(vm.get_global("x"), Some(Value::I64(5))),
                "frame-0 global preserved across append + re-enter"
            );
        });
    }

    /// LV3 (Issues #9199/#11250): a brand-new generic function definition compiles its
    /// body and APPENDS it to the held live VM (no fresh `Vm::new_program`), and
    /// the function is immediately callable and returns the right value.
    #[test]
    fn definition_delta_appends_to_live_vm() {
        with_large_stack(|| {
            let mut s = persistent();
            // Warm the session so a live VM is parked and the prefix is in sync.
            assert!(s.eval("wa9199 = 5").success);
            assert!(s.eval("wa9199 + 1").success);
            assert!(s.has_live_vm(), "prior eval parks a VM");

            // Define a brand-new generic → compiled-definition live-append.
            let def = s.eval("newgen9199(x) = x + 1");
            assert!(def.success, "{:?}", def.error);
            assert_eq!(
                s.last_vm_build_nanos(),
                Some(0),
                "a new-generic definition must install on the live VM, not a fresh build"
            );

            // The just-defined function is callable and correct (this call
            // full-recompiles to re-sync the stale prefix, but the RESULT is the
            // point).
            let call = s.eval("newgen9199(41)");
            assert!(call.success, "{:?}", call.error);
            assert_eq!(i64_of(&call), Some(42));
        });
    }

    /// LV3 (Issue #9199): a definition-delta that also calls a BASE function in
    /// its body (`sqrt`) still appends cleanly and is correct; and a two-function
    /// definition installs both.
    #[test]
    fn definition_delta_base_call_and_multi_fn() {
        with_large_stack(|| {
            let mut s = persistent();
            assert!(s.eval("wb9199 = 1").success);
            assert!(s.eval("wb9199 + 1").success);

            let def = s.eval("basecall9199(x) = sqrt(x) + 1.0");
            assert!(def.success, "{:?}", def.error);
            assert_eq!(
                s.last_vm_build_nanos(),
                Some(0),
                "single base-calling def is live"
            );
            let call = s.eval("basecall9199(4.0)");
            assert!(call.success, "{:?}", call.error);
            assert!(matches!(call.value, Some(Value::F64(v)) if (v - 3.0).abs() < 1e-9));

            // A fresh two-function definition, after the prefix re-synced above.
            assert!(s.eval("wc9199 = 2").success); // re-sync + re-park
            let two = s.eval("mfa9199(x) = x + 1\nmfb9199(x) = x * 10");
            assert!(two.success, "{:?}", two.error);
            assert_eq!(i64_of(&s.eval("mfa9199(4)")), Some(5));
            assert_eq!(i64_of(&s.eval("mfb9199(4)")), Some(40));
        });
    }

    /// LV3 (Issue #9199): a SAME-EVAL cross reference — a function whose body
    /// calls another function defined in the SAME input — is installed correctly:
    /// both bodies append at aligned live indices `[P, P+1]`, so the caller's
    /// `Call(P)` resolves to the callee installed just before it. Exercises the
    /// "new function installed BEFORE code referencing it runs" invariant AND the
    /// gate accepting a function-index reference in the new batch `[P, P+u)`.
    #[test]
    fn definition_delta_same_batch_cross_reference() {
        with_large_stack(|| {
            let mut s = persistent();
            assert!(s.eval("wd9199 = 1").success);
            assert!(s.eval("wd9199 + 1").success);
            // `hh9199` calls `gg9199`, both brand-new in this one eval.
            let def = s.eval("gg9199(x) = x + 1\nhh9199(x) = gg9199(x) * 10");
            assert!(def.success, "{:?}", def.error);
            assert_eq!(
                s.last_vm_build_nanos(),
                Some(0),
                "a same-batch cross-referencing definition still lives"
            );
            // hh(4) = (4+1)*10 = 50; gg(4) = 5.
            assert_eq!(i64_of(&s.eval("hh9199(4)")), Some(50));
            assert_eq!(i64_of(&s.eval("gg9199(4)")), Some(5));
        });
    }

    /// LV3 gate (Issue #9199): a definition that is NOT a brand-new generic —
    /// a redefinition, a same-name method extension, or a Base extension — must
    /// NOT take the live-append path; it routes to the full recompile, and the
    /// result matches upstream-reviewed goldens. This is the outer safety layer
    /// above the compile-side `input_defines_only_new_generic_functions` and the
    /// extraction gate.
    #[test]
    fn non_new_generic_definitions_match_upstream_goldens() {
        with_large_stack(|| {
            // Redefinition (same signature) and method extension (new signature)
            // of a prior user generic, plus a Base extension.
            let seqs: &[&[&str]] = &[
                &[
                    "rdf9199(x) = x + 1",
                    "rdf9199(5)",
                    "rdf9199(x) = x + 100",
                    "rdf9199(5)",
                ],
                &[
                    "mex9199(x) = x + 1",
                    "mex9199(5)",
                    "mex9199(x, y) = x + y",
                    "mex9199(2, 3)",
                ],
            ];
            for (seq, expected) in seqs.iter().zip([105, 5]) {
                let mut session = persistent();
                let mut last = None;
                for input in *seq {
                    let result = session.eval(input);
                    assert!(result.success, "{input:?}: {:?}", result.error);
                    last = i64_of(&result).or(last);
                }
                assert_eq!(last, Some(expected));
            }
        });
    }

    /// LV3 world-age (Issue #9199 / #9400 / #8452): a redefinition of a
    /// previously live-appended generic is NOT retroactive across evals — a value
    /// computed and saved BEFORE the redefinition keeps the old result, and a new
    /// call site sees the new method. Both models agree (verified against
    /// upstream `julia`: 11 then 110, saved stays 11).
    #[test]
    fn definition_append_worldage_not_retroactive() {
        with_large_stack(|| {
            {
                let mut s = REPLSession::new(0);
                assert!(s.eval("wg9199lv3 = 1").success); // warm (park a VM)
                assert!(s.eval("wa9199lv3(x) = x + 1").success); // new generic
                assert!(s.eval("saved9199lv3 = wa9199lv3(10)").success);
                assert!(s.eval("wa9199lv3(x) = x + 100").success); // redefinition
                assert_eq!(i64_of(&s.eval("wa9199lv3(10)")), Some(110));
                assert_eq!(i64_of(&s.eval("saved9199lv3")), Some(11));
            }
        });
    }

    /// Issue #9784 Slice 2: a catchable error commits exactly the reached
    /// method-identity activation groups. The first replacement remains live;
    /// the later extension, whose marker was not reached, stays absent.
    #[test]
    fn method_mutation_error_commits_only_reached_identity_prefix_9784() {
        with_large_stack(|| {
            let mut session = REPLSession::new(0);
            assert!(session.eval("prefix_method_9784(x::Int64) = x + 1").success);

            let failed = session.eval(
                "prefix_method_9784(x::Int64) = x + 100; error(\"stop identity prefix\"); prefix_method_9784(x::Float64) = x + 0.5",
            );
            assert!(!failed.success);
            assert!(
                session.has_live_vm(),
                "a catchable runtime error must retain the live VM"
            );
            assert_eq!(
                i64_of(&session.eval("prefix_method_9784(1)")),
                Some(101),
                "the reached replacement must remain active"
            );
            let absent = session.eval("prefix_method_9784(1.0)");
            assert!(!absent.success, "the unreached extension must stay absent");
            assert!(
                absent
                    .error
                    .as_deref()
                    .is_some_and(|error| error.contains("MethodError")),
                "expected MethodError, got {:?}",
                absent.error
            );
        });
    }

    /// Re-entering twice in a row keeps compounding on the SAME live VM: the
    /// second appended `main` sees the first one's world-preserving frame-0.
    #[test]
    fn reenter_appended_main_is_repeatable() {
        with_large_stack(|| {
            let mut session = REPLSession::new(0);
            assert!(session.eval("g = 100").success);
            let mut vm = session.live_vm.take().expect("live VM held");

            for expected in [110_i64, 110] {
                let main = vec![
                    Instr::LoadGlobalAny("g".to_string()),
                    Instr::PushI64(10),
                    Instr::AddI64,
                    Instr::ReturnAny,
                ];
                let sm = vec![None; main.len()];
                vm.reenter_appended_main(&main, &sm, StableRng::new(1));
                let out = vm.run().expect("run");
                assert!(matches!(out, Value::I64(v) if v == expected), "got {out:?}");
                // `g` is only READ, never stored, so it stays 100 each time.
                assert!(matches!(vm.get_global("g"), Some(Value::I64(100))));
            }
        });
    }

    /// The `REPLSession` live-VM hold is the LV1 foundation: a persistent eval
    /// parks its VM and `reset()` drops it (reset == fresh session).
    #[test]
    fn live_vm_hold_and_reset_semantics() {
        with_large_stack(|| {
            let mut session = REPLSession::new(0);
            assert!(!session.has_live_vm(), "no VM before the first eval");
            assert!(session.eval("y = 1").success);
            assert!(session.has_live_vm(), "persistent eval parks a VM");
            session.reset();
            assert!(!session.has_live_vm(), "reset() drops the live VM");
        });
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod lv2_live_delta_tests {
    //! LV2 (Issue #9199) end-to-end tests of the WIRED live-append path: an
    //! eligible expression / reassignment / new-global delta compiles as a
    //! relocatable delta main (`repl_relocatable_delta_compile`) and re-enters the
    //! held live VM instead of rebuilding it. `last_vm_build_nanos() == Some(0)` is
    //! the observable signature that the live path (not the fresh Vm::new_program
    //! path) ran. Correctness is checked against upstream-reviewed values; the
    //! golden harness pins the same invariant over a larger corpus.
    use super::*;

    fn with_large_stack<F: FnOnce() + Send + 'static>(f: F) {
        std::thread::Builder::new()
            .stack_size(64 * 1024 * 1024)
            .spawn(f)
            .unwrap()
            .join()
            .unwrap();
    }

    fn persistent() -> REPLSession {
        REPLSession::new(0)
    }

    fn i64_of(r: &REPLResult) -> Option<i64> {
        match &r.value {
            Some(Value::I64(v)) => Some(*v),
            _ => None,
        }
    }

    /// A pure expression delta reads live globals by name/slot and runs on the
    /// live VM (no fresh build), leaving frame-0 intact.
    #[test]
    fn expression_delta_runs_on_live_vm() {
        with_large_stack(|| {
            let mut s = persistent();
            assert!(s.eval("a = 5").success);
            assert!(s.eval("b = 7").success);
            let r = s.eval("a + b");
            assert!(r.success, "{:?}", r.error);
            assert_eq!(i64_of(&r), Some(12));
            // The live path reuses the parked VM — no `Vm::new_program`.
            assert_eq!(
                s.last_vm_build_nanos(),
                Some(0),
                "expression delta must run on the live VM, not a fresh build"
            );
            assert!(s.has_live_vm());
        });
    }

    /// HOF deltas (`ntuple(i -> i, 2)`, `sprint(io -> …)`, `map(x -> …)`) lift
    /// marker-less helper bodies. Those helpers must install immediately on the
    /// held VM, just like an immediately-invoked main-inline closure, rather than
    /// forcing a fresh rebuild (Issue #9784).
    #[test]
    fn hof_helpers_install_on_live_vm_9784() {
        with_large_stack(|| {
            for expr in [
                "ntuple(i -> i, 2)",
                "sprint(io -> print(io, \"x\"))",
                "map(x -> x + 1, [1, 2])",
            ] {
                let mut p = persistent();
                assert!(p.eval("z = 1").success);
                let rp = p.eval(expr);
                assert!(rp.success, "persistent {expr:?} errored: {:?}", rp.error);
                assert_eq!(
                    p.last_vm_build_nanos(),
                    Some(0),
                    "{expr:?} must install its marker-less helper on the held VM"
                );
            }

            let mut p = persistent();
            assert!(p.eval("z = 1").success);
            let rp = p.eval("(() -> 41)()");
            assert!(rp.success, "inline closure errored: {:?}", rp.error);
            assert_eq!(i64_of(&rp), Some(41));
            assert_eq!(
                p.last_vm_build_nanos(),
                Some(0),
                "a relocatable main-inline helper must install on the live VM"
            );
        });
    }

    /// THE slot-correctness test the LV1 memory note calls out: define g1..gN,
    /// then a delta that reassigns g_k and reads it must land on the CORRECT live
    /// frame-0 slot (not collide with another global's slot). Also verifies the
    /// other globals are untouched.
    #[test]
    fn reassigned_global_lands_on_correct_live_slot() {
        with_large_stack(|| {
            let mut s = persistent();
            for (name, val) in [("g1", 1), ("g2", 2), ("g3", 3), ("g4", 4), ("g5", 5)] {
                let r = s.eval(&format!("{name} = {val}"));
                assert!(r.success, "{:?}", r.error);
            }
            // Reassign a middle global on the live VM.
            let r = s.eval("g3 = 42");
            assert!(r.success, "{:?}", r.error);
            assert_eq!(i64_of(&r), Some(42));
            assert_eq!(
                s.last_vm_build_nanos(),
                Some(0),
                "reassignment delta must run on the live VM"
            );
            // g3 updated to the correct slot; every other global intact.
            for (name, want) in [("g1", 1), ("g2", 2), ("g3", 42), ("g4", 4), ("g5", 5)] {
                let r = s.eval(name);
                assert!(r.success, "read {name}: {:?}", r.error);
                assert_eq!(i64_of(&r), Some(want), "global {name} wrong after reassign");
            }
        });
    }

    /// A delta that reassigns a global using its own prior value (read-then-store
    /// on the same live slot) accumulates correctly across many evals.
    #[test]
    fn read_modify_write_global_accumulates() {
        with_large_stack(|| {
            let mut s = persistent();
            assert!(s.eval("counter = 0").success);
            for expected in 1..=5 {
                let r = s.eval("counter = counter + 1");
                assert!(r.success, "{:?}", r.error);
                assert_eq!(i64_of(&r), Some(expected));
                assert_eq!(s.last_vm_build_nanos(), Some(0));
            }
            let r = s.eval("counter");
            assert_eq!(i64_of(&r), Some(5));
        });
    }

    /// A brand-new global bound by a delta grows frame-0 in place and reads a
    /// prior global — the LV2 frame-0 growth path.
    #[test]
    fn new_global_delta_grows_frame0() {
        with_large_stack(|| {
            let mut s = persistent();
            assert!(s.eval("base = 100").success);
            let r = s.eval("derived = base + 23");
            assert!(r.success, "{:?}", r.error);
            assert_eq!(i64_of(&r), Some(123));
            assert_eq!(
                s.last_vm_build_nanos(),
                Some(0),
                "new-global delta on live VM"
            );
            // Both the new and the prior global read back correctly next eval.
            assert_eq!(i64_of(&s.eval("derived")), Some(123));
            assert_eq!(i64_of(&s.eval("base")), Some(100));
        });
    }

    /// The live path matches upstream-reviewed values over a mixed sequence of
    /// expressions, new bindings, and reassignments.
    #[test]
    fn live_path_matches_legacy_over_sequence() {
        with_large_stack(|| {
            let seq = [
                "x = 3",
                "y = 4",
                "x + y",
                "x = x * 10",
                "z = x - y",
                "z",
                "x",
                "y",
            ];
            let mut live = persistent();
            let expected = [
                Some(3),
                Some(4),
                Some(7),
                Some(30),
                Some(26),
                Some(26),
                Some(30),
                Some(4),
            ];
            for (input, expected_value) in seq.into_iter().zip(expected) {
                let rp = live.eval(input);
                assert!(rp.success, "{input:?}: {:?}", rp.error);
                assert_eq!(i64_of(&rp), expected_value, "value mismatch on {input:?}");
            }
        });
    }

    /// Repro of the semantics_matrix divergence: a top-level `let x = 99`
    /// shadowing a live global `x` must NOT corrupt the global (Issue #9199 LV2).
    #[test]
    fn let_shadowing_global_does_not_corrupt() {
        with_large_stack(|| {
            // Isolated: no intervening live-path steps.
            let mut s0 = persistent();
            assert_eq!(i64_of(&s0.eval("x = 10")), Some(10));
            let rl0 = s0.eval("let\n    x = 99\n    x\nend");
            assert!(rl0.success, "{:?}", rl0.error);
            let rx0 = s0.eval("x");
            assert!(
                rx0.success,
                "isolated: reading x after let: {:?}",
                rx0.error
            );
            assert_eq!(i64_of(&rx0), Some(10), "isolated global x must survive let");

            // With an intervening live-path step (the semantics_matrix shape).
            let mut s = persistent();
            assert_eq!(i64_of(&s.eval("x = 10")), Some(10));
            assert_eq!(
                i64_of(&s.eval("for i in 1:2\n    x += i\nend\nx")),
                Some(13)
            );
            let rlet = s.eval("let\n    x = 99\n    x\nend");
            assert!(rlet.success, "{:?}", rlet.error);
            assert_eq!(i64_of(&rlet), Some(99), "let value");
            let rx = s.eval("x");
            assert!(rx.success, "reading x after let: {:?}", rx.error);
            assert_eq!(i64_of(&rx), Some(13), "global x must survive the let");
        });
    }

    /// Issue #11569 / #9784: transient lexical names belong to the VM's lexical
    /// environment, never to frame 0. Repeated seeded delta compiles must
    /// therefore leave both the physical global-slot layout and the set of
    /// defined module bindings exactly stable.
    #[test]
    fn hard_scope_transient_names_do_not_grow_global_slots_11569() {
        with_large_stack(|| {
            let mut s = persistent();
            assert!(s.eval("hard_scope_slot_seed11569 = 7").success);
            assert!(s.eval("hard_scope_loop_sink11569 = 0").success);

            let seed_vm = s.live_vm.as_ref();
            assert!(seed_vm.is_some(), "the seed eval must park a VM");
            let Some(seed_vm) = seed_vm else {
                return;
            };
            let before_slots = seed_vm.global_slot_names().to_vec();
            let before_defined = seed_vm.defined_repl_global_names();

            for index in 0_i64..32 {
                let source = format!(
                    "let hard_scope_transient_{index}_11569 = {index}\n  hard_scope_transient_{index}_11569\nend"
                );
                let result = s.eval(&source);
                assert!(result.success, "`{source}`: {:?}", result.error);
                assert_eq!(i64_of(&result), Some(index));
                assert_eq!(s.last_vm_build_nanos(), Some(0));
                assert!(s.has_live_vm());
            }

            for index in 0_i64..16 {
                let source = format!(
                    "for hard_scope_loop_transient_{index}_11569 in 1:1\n  global hard_scope_loop_sink11569 = hard_scope_loop_transient_{index}_11569\nend"
                );
                let result = s.eval(&source);
                assert!(result.success, "`{source}`: {:?}", result.error);
                assert_eq!(s.last_vm_build_nanos(), Some(0));
                assert!(s.has_live_vm());
            }

            let vm = s.live_vm.as_ref();
            assert!(vm.is_some(), "the last let must park the VM");
            let Some(vm) = vm else {
                return;
            };
            assert_eq!(
                vm.global_slot_names(),
                before_slots,
                "lexical stores must never append frame-0 slots"
            );
            assert_eq!(
                vm.defined_repl_global_names(),
                before_defined,
                "transient lexical declarations must never become module bindings"
            );
        });
    }

    /// A brand-new generic definition now takes the LV3 compiled live-append
    /// path (Issue #9199) — vm-build 0 — even when its body reads a module
    /// global; the function is then correct, and a following expression resumes
    /// the live path. (Before LV3 a definition always full-recompiled; this test
    /// pinned that older behavior and is updated to the new one.)
    #[test]
    fn definition_eval_appends_then_delta_resumes() {
        with_large_stack(|| {
            let mut s = persistent();
            assert!(s.eval("qq9199 = 2").success);
            // A brand-new generic whose body reads the global `qq9199` → the LV3
            // compiled-definition live-append (no fresh `Vm::new_program`).
            let rdef = s.eval("fdef9199(n) = n + qq9199");
            assert!(rdef.success, "{:?}", rdef.error);
            assert_eq!(
                s.last_vm_build_nanos(),
                Some(0),
                "a brand-new generic definition takes the LV3 live-append path"
            );
            // The new function reads the live global correctly without the old
            // one-eval full-refresh detour: the definition append advanced the
            // reusable compiler snapshot together with the VM.
            let r = s.eval("fdef9199(40)");
            assert!(r.success, "{:?}", r.error);
            assert_eq!(i64_of(&r), Some(42));
            assert_eq!(s.last_vm_build_nanos(), Some(0));

            // A pure arithmetic expression remains on the live path.
            let r2 = s.eval("qq9199 + 100");
            assert!(r2.success, "{:?}", r2.error);
            assert_eq!(i64_of(&r2), Some(102));
            assert_eq!(
                s.last_vm_build_nanos(),
                Some(0),
                "an arithmetic expression after a definition resumes the live path"
            );
        });
    }

    /// Issue #9784: definition deltas advance the reusable compiler snapshot,
    /// so consecutive function and struct definitions remain live-appended.
    /// This directly guards the function-index and type-id alignment between
    /// the compiler prefix and held VM; the final expression reaches every
    /// definition without an intervening fresh VM rebuild.
    #[test]
    fn consecutive_definition_deltas_advance_persistent_snapshot_9784() {
        with_large_stack(|| {
            let mut s = persistent();
            assert!(s.eval("seed9784 = 10").success);

            for source in [
                "first9784(x) = x + seed9784",
                // Leave an expression-main gap between definition snapshots;
                // the next snapshot must preserve its live code offset.
                "first9784(1)",
                "second9784(x) = first9784(x) * 2",
                "struct Left9784\n  value::Int\nend",
                "struct Right9784\n  value::Left9784\nend",
            ] {
                let result = s.eval(source);
                assert!(result.success, "`{source}`: {:?}", result.error);
                assert_eq!(
                    s.last_vm_build_nanos(),
                    Some(0),
                    "`{source}` must append without a stale-prefix full refresh"
                );
            }

            let result = s.eval("Right9784(Left9784(second9784(11))).value.value");
            assert!(result.success, "{:?}", result.error);
            assert_eq!(i64_of(&result), Some(42));
            assert_eq!(s.last_vm_build_nanos(), Some(0));
        });
    }

    /// Advancing the snapshot must not freeze a baked undefined-name trap in a
    /// prior caller. Defining that missing callee retains the historical full
    /// refresh, which recompiles the caller and repairs the forward reference.
    #[test]
    fn definition_delta_refreshes_prior_forward_reference_9784() {
        with_large_stack(|| {
            let mut s = persistent();
            assert!(s.eval("seed_forward9784 = 1").success);

            let caller = s.eval("caller_forward9784(x) = callee_forward9784(x) + 1");
            assert!(caller.success, "{:?}", caller.error);
            assert_eq!(s.last_vm_build_nanos(), Some(0));

            let callee = s.eval("callee_forward9784(x) = x * 10");
            assert!(callee.success, "{:?}", callee.error);
            assert_ne!(
                s.last_vm_build_nanos(),
                Some(0),
                "the missing callee must refresh the frozen caller"
            );

            let result = s.eval("caller_forward9784(5)");
            assert!(result.success, "{:?}", result.error);
            assert_eq!(i64_of(&result), Some(51));
        });
    }

    /// A successful live definition is active in both the VM and the advanced
    /// compiler snapshot. A later expression error recovers that same VM, so the
    /// installed method remains callable without rebuilding or regressing to the
    /// compiler's pre-activation world sentinel.
    #[test]
    fn advanced_definition_snapshot_survives_live_vm_error_recovery_9784() {
        with_large_stack(|| {
            let mut s = persistent();
            assert!(s.eval("seed_drop9784 = 2").success);
            assert!(s.eval("after_drop9784(x) = x + seed_drop9784").success);
            assert_eq!(s.last_vm_build_nanos(), Some(0));

            let failed = s.eval("error(\"recover live vm\")");
            assert!(!failed.success);
            assert!(s.has_live_vm());

            let result = s.eval("after_drop9784(40)");
            assert!(result.success, "{:?}", result.error);
            assert_eq!(i64_of(&result), Some(42));
            assert_eq!(s.last_vm_build_nanos(), Some(0));
        });
    }

    /// LV4 (Issue #9199): a brand-new CONCRETE struct definition takes the
    /// compiled type live-append path (`install_appended_types`, no fresh
    /// `Vm::new_program`), and the struct is usable — constructed + field-read —
    /// on a later eval. Mirrors the LV3 function analog.
    #[test]
    fn struct_definition_delta_appends_to_live_vm_9199() {
        with_large_stack(|| {
            let mut s = persistent();
            // Warm: a global so a live VM is parked for the struct def to append to.
            assert!(s.eval("qqs9199 = 5").success);
            // A brand-new non-parametric struct → the LV4 compiled type live-append.
            let rdef = s.eval("struct Pt9199\n  x::Int\nend");
            assert!(rdef.success, "{:?}", rdef.error);
            assert_eq!(
                s.last_vm_build_nanos(),
                Some(0),
                "a brand-new struct definition takes the LV4 live type-append path"
            );
            // A bare definition echoes no value, matching upstream.
            assert!(matches!(rdef.value, None | Some(Value::Nothing)));
            // Construct + read a field on a later live eval.
            let inst = s.eval("ps9199 = Pt9199(7)");
            assert!(inst.success, "{:?}", inst.error);
            let field = s.eval("ps9199.x");
            assert!(field.success, "{:?}", field.error);
            assert_eq!(i64_of(&field), Some(7));
        });
    }

    /// LV4 (Issue #9199): a SINGLE eval that defines a struct AND immediately
    /// constructs + uses it runs entirely on the live VM — the type is installed
    /// (`install_appended_types`) BEFORE the appended main runs its `NewStruct`, so
    /// the aligned `type_id` resolves. vm-build is 0 (live re-enter).
    #[test]
    fn struct_def_and_use_same_eval_live_append_9199() {
        with_large_stack(|| {
            let mut s = persistent();
            assert!(s.eval("qw9199 = 1").success);
            let r = s.eval("struct Pw9199\n  a::Int\n  b::Int\nend\np9199w = Pw9199(3, 4)\np9199w.a + p9199w.b");
            assert!(r.success, "{:?}", r.error);
            assert_eq!(i64_of(&r), Some(7));
            assert_eq!(
                s.last_vm_build_nanos(),
                Some(0),
                "a same-eval struct def + use runs on the live VM (type installed before main)"
            );
        });
    }

    /// LV4 (Issue #9199): a struct REDEFINITION is NOT eligible for the live
    /// type-append (its name is already a prefix/prior type → changing its
    /// `type_id`/layout must recompile every prior `NewStruct` reference), so it
    /// routes to the full recompile — and the redefined layout takes effect (the
    /// new 2-field version constructs and its new field reads back).
    #[test]
    fn struct_redefinition_full_recompiles_9199() {
        with_large_stack(|| {
            let mut s = persistent();
            assert!(s.eval("qr9199 = 1").success);
            // First definition (1 field) → live-append.
            assert!(s.eval("struct Pr9199\n  x::Int\nend").success);
            // Resync the stale prefix so the redefinition below sees Pr9199 as an
            // existing type (routing it to the full recompile, not a live-append).
            assert!(s.eval("qr9199 + 1").success);
            // Redefinition (2 fields) → NOT a brand-new type → full recompile.
            let redef = s.eval("struct Pr9199\n  x::Int\n  y::Int\nend");
            assert!(redef.success, "{:?}", redef.error);
            // The redefined 2-field layout is active: construct with 2 args + read
            // the NEW field.
            let use_new = s.eval("Pr9199(10, 20).y");
            assert!(use_new.success, "{:?}", use_new.error);
            assert_eq!(i64_of(&use_new), Some(20));
        });
    }

    /// LV5 (Issue #9199): a delta that only REFERENCES a prior simple user module
    /// re-enters the module-realized parked VM (vm-build 0, no fresh
    /// `Vm::new_program`), so the module's mutable const state persists in the VM
    /// across evals WITHOUT `restore_module_globals` — the structural close of
    /// #5296 for the covered subset. The module DEFINITION eval itself takes the
    /// full recompile (it realizes + parks the VM); the reference evals are live.
    #[test]
    fn module_reference_delta_appends_to_live_vm_9199() {
        with_large_stack(|| {
            let mut s = persistent();
            // Realize a simple user module (const array + a mutating function).
            let def = s.eval(
                "module Log9199\n  const entries = Int[]\n  bump() = (push!(entries, length(entries) + 1); length(entries))\nend",
            );
            assert!(def.success, "{:?}", def.error);
            assert!(s.has_live_vm(), "the module-realized VM is parked");

            // First reference: takes the LIVE path (vm-build 0), mutates module state.
            let b1 = s.eval("Log9199.bump()");
            assert!(b1.success, "{:?}", b1.error);
            assert_eq!(i64_of(&b1), Some(1));
            assert_eq!(
                s.last_vm_build_nanos(),
                Some(0),
                "a module-reference delta re-enters the parked VM (LV5)"
            );

            // Second reference: state persisted IN the live VM (no restore fakery).
            let b2 = s.eval("Log9199.bump()");
            assert!(b2.success, "{:?}", b2.error);
            assert_eq!(i64_of(&b2), Some(2));
            assert_eq!(s.last_vm_build_nanos(), Some(0));

            // Read the accumulated module const on the live path.
            let len = s.eval("length(Log9199.entries)");
            assert!(len.success, "{:?}", len.error);
            assert_eq!(i64_of(&len), Some(2));
            assert_eq!(s.last_vm_build_nanos(), Some(0));
        });
    }

    /// LV5 / Issue #11569: module const state stays coherent while a hard-scope
    /// `let` executes directly on the module-bearing live VM. The lexical owner
    /// must not disturb either frame 0 or the realized module state.
    #[test]
    fn module_state_coherent_across_hard_scope_live_delta_9199() {
        with_large_stack(|| {
            let mut s = persistent();
            assert!(
                s.eval(
                    "module Cn9199\n  const xs = Int[]\n  add() = (push!(xs, 1); length(xs))\nend"
                )
                .success
            );
            // Live-path mutation → xs == [1].
            let a = s.eval("Cn9199.add()");
            assert_eq!(i64_of(&a), Some(1));
            assert_eq!(s.last_vm_build_nanos(), Some(0));
            // The hard-scope lexical owner now runs on, and then returns to, the
            // same parked VM without touching the module's state.
            let lt = s.eval("let z9199 = 7\n  z9199\nend");
            assert!(lt.success, "{:?}", lt.error);
            assert_eq!(
                s.last_vm_build_nanos(),
                Some(0),
                "a hard-scope let must reuse the live VM"
            );
            assert!(s.has_live_vm());
            // Next mutation continues from the preserved state → xs == [1, 1].
            let b = s.eval("Cn9199.add()");
            assert!(b.success, "{:?}", b.error);
            assert_eq!(
                i64_of(&b),
                Some(2),
                "module state survived the hard-scope live delta"
            );
        });
    }

    /// LV5 (Issue #9199): the eligibility gate is FAIL-CLOSED. A module that is
    /// NOT a simple persistable user module is never admitted to the live delta
    /// path — every reference to it takes the full recompile (vm-build > 0), so
    /// the LV5 append is never attempted for a module whose realization the
    /// carried surface does not cover (LV5b). Since Issue #9723 widened the gate
    /// to struct-bearing modules and submodules, this pins a kind that remains
    /// fail-closed: a module-level MACRO (a delta's lowering would need the
    /// macro body, which the carried name-set surface does not provide).
    #[test]
    fn non_simple_module_session_stays_on_full_recompile_9199() {
        with_large_stack(|| {
            let mut s = persistent();
            // A module with a module-level macro is NOT simple-persistable (LV5b).
            let def =
                s.eval("module Mz9199\n  macro nine9199()\n    :(9)\n  end\n  make() = 9\nend");
            assert!(def.success, "{:?}", def.error);
            // Reference it: must NOT take the live path (fail-closed) and must still
            // work via the full recompile.
            let r = s.eval("Mz9199.make()");
            assert!(r.success, "{:?}", r.error);
            assert_eq!(i64_of(&r), Some(9));
            assert_ne!(
                s.last_vm_build_nanos(),
                Some(0),
                "a module with a module-level macro stays on the full recompile path (fail-closed)"
            );
        });
    }

    /// Issue #9989 / #9729: `if` branches at module top level do not introduce a
    /// local scope, so a `const` inside `if true ... end` is a module binding. The
    /// resolution collector and state-mirror collector must both see it. Persistent
    /// may therefore take the LV5 live path for references to such a module, and a
    /// later full-recompile fallback must restore the current live-mutated state.
    /// This pins the upstream-golden and live→full state-coherence invariants while
    /// proving the old fail-closed expectation from #9199 is no longer required for
    /// `Stmt::If` bindings.
    #[test]
    fn if_wrapped_module_const_stays_legacy_equiv_9980() {
        with_large_stack(|| {
            let seq = [
                "module Mif9199\n  if true\n    const xs = Int[]\n  end\n  bump() = (push!(xs, 1); length(xs))\nend",
                "Mif9199.bump()",
                "Mif9199.bump()",
                "let q9199 = 7\n  q9199\nend",
                "Mif9199.bump()",
                "length(Mif9199.xs)",
            ];
            let run = || {
                let mut s = REPLSession::new(0);
                let mut vals = Vec::new();
                let mut took_live = false;
                for src in seq {
                    let r = s.eval(src);
                    assert!(r.success, "`{src}`: {:?}", r.error);
                    vals.push(i64_of(&r));
                    took_live |= s.last_vm_build_nanos() == Some(0);
                }
                (vals, took_live)
            };
            let (values, took_live) = run();
            assert_eq!(
                values,
                vec![None, Some(1), Some(2), Some(7), Some(3), Some(3)],
                "if-wrapped module const state should survive live and full paths"
            );
            // Since #9729, the state mirror tracks module-top-level `if` branches,
            // so the module is safe to admit to the live path.
            assert!(
                took_live,
                "an if-wrapped-const module should be eligible for the live path"
            );
        });
    }
}

/// LV5 mirror-coverage table (Issue #9996; prevention for #9989 / PR #9995).
///
/// `module_bindings_fully_mirrorable` gates the LV5 live path on the invariant
/// `mirror ⊇ resolution` between two collectors that walk a module body with
/// asymmetric coverage:
/// - RESOLUTION: `collect_module_body_binding_names` (`compile/collect.rs`)
/// - MIRROR:     `collect_assign_vars_in_stmts` (this file)
///
/// This table pins the exact classification of each module-body statement shape
/// per collector. When a collector's coverage changes INTENTIONALLY (as #9729
/// did for module-top-level `if`), the corresponding row here must be updated
/// explicitly, in the same change, together with the ADR LV5/LV5b wording
/// (`docs/vm/ADR_REPL_EVAL_MODEL.md`). A coverage change that is NOT accompanied
/// by an expectation update fails here — instead of surfacing as a stale
/// path-policy assertion in an unrelated test, which was the #9989 failure mode.
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod lv5_mirror_coverage_tests_9996 {
    use super::*;
    use crate::ir::core::Block;
    use std::collections::HashSet;

    fn sp() -> Span {
        Span::new(0, 0, 0, 0, 0, 0)
    }

    fn assign(name: &str) -> Stmt {
        Stmt::Assign {
            var: name.to_string(),
            value: Expr::Literal(Literal::Int(1), sp()),
            span: sp(),
        }
    }

    fn blk(stmts: Vec<Stmt>) -> Block {
        Block { stmts, span: sp() }
    }

    fn module_with_body(stmts: Vec<Stmt>) -> Module {
        Module {
            name: "M9996".to_string(),
            is_bare: false,
            is_package_origin: false,
            is_base_origin: false,
            functions: Vec::new(),
            structs: Vec::new(),
            abstract_types: Vec::new(),
            primitive_types: Vec::new(),
            type_aliases: Vec::new(),
            submodules: Vec::new(),
            usings: Vec::new(),
            macros: Vec::new(),
            exports: Vec::new(),
            publics: Vec::new(),
            body: blk(stmts),
            span: sp(),
        }
    }

    /// One row of the coverage table: the binding `x = 1` planted inside one
    /// module-body statement shape, with the expected view of BOTH collectors
    /// and the resulting `module_bindings_fully_mirrorable` classification.
    struct Row {
        shape: &'static str,
        stmts: Vec<Stmt>,
        /// Does `collect_module_body_binding_names` (RESOLUTION) see `x`?
        resolution_sees: bool,
        /// Does `collect_assign_vars_in_stmts` (STATE MIRROR) see `x`?
        mirror_sees: bool,
        /// Expected classification: `true` = LIVE-path eligible
        /// (`module_bindings_fully_mirrorable`), `false` = routes to the
        /// full-recompile fallback (LV5b).
        live_path_eligible: bool,
    }

    /// The coverage table (Issue #9996). Update a row ONLY together with the
    /// collector change that justifies it — that update is the explicit
    /// expectation change this prevention test exists to force.
    fn coverage_table() -> Vec<Row> {
        vec![
            // Plain top-level assignment: both collectors, live.
            Row {
                shape: "Assign",
                stmts: vec![assign("x")],
                resolution_sees: true,
                mirror_sees: true,
                live_path_eligible: true,
            },
            // `begin ... end` introduces no scope at module top level (also the
            // lowered shape of `const x = 1`): both collectors, live.
            Row {
                shape: "Block",
                stmts: vec![Stmt::Block(blk(vec![assign("x")]))],
                resolution_sees: true,
                mirror_sees: true,
                live_path_eligible: true,
            },
            // Module-top-level `if` introduces no scope (Issue #7917). The
            // mirror walks it since Issue #9729, so it is LIVE — the exact
            // expectation #9989 was about.
            Row {
                shape: "If (module top level)",
                stmts: vec![Stmt::If {
                    condition: Expr::Literal(Literal::Bool(true), sp()),
                    then_branch: blk(vec![assign("x")]),
                    else_branch: None,
                    span: sp(),
                }],
                resolution_sees: true,
                mirror_sees: true,
                live_path_eligible: true,
            },
            // Assignment in expression position: resolution sees it, the
            // mirror does NOT → fail-closed to the full recompile (LV5b).
            Row {
                shape: "AssignExpr",
                stmts: vec![Stmt::Expr {
                    expr: Expr::AssignExpr {
                        var: "x".to_string().into(),
                        value: Box::new(Expr::Literal(Literal::Int(1), sp())),
                        span: sp(),
                    },
                    span: sp(),
                }],
                resolution_sees: true,
                mirror_sees: false,
                live_path_eligible: false,
            },
            // Empty-binding `LetBlock` (macro-expanded `begin`/`quote` wrapper,
            // no fresh scope): resolution sees through it, the mirror does NOT
            // → fail-closed to the full recompile (LV5b).
            Row {
                shape: "empty LetBlock",
                stmts: vec![Stmt::Expr {
                    expr: Expr::LetBlock {
                        bindings: Vec::new(),
                        body: blk(vec![assign("x")]),
                        span: sp(),
                    },
                    span: sp(),
                }],
                resolution_sees: true,
                mirror_sees: false,
                live_path_eligible: false,
            },
            // Control row: `for` DOES introduce a local scope, so NEITHER
            // collector may leak `x` as a module binding; the module is then
            // vacuously mirrorable (no resolvable binding at all).
            Row {
                shape: "For (local scope)",
                stmts: vec![Stmt::For {
                    var: "i".to_string(),
                    start: Expr::Literal(Literal::Int(1), sp()),
                    end: Expr::Literal(Literal::Int(1), sp()),
                    step: None,
                    body: blk(vec![assign("x")]),
                    span: sp(),
                }],
                resolution_sees: false,
                mirror_sees: false,
                live_path_eligible: true,
            },
        ]
    }

    fn resolution_names(module: &Module) -> HashSet<String> {
        let mut names = HashSet::new();
        repl_support::collect_module_body_binding_names(&module.body, &mut names);
        names
    }

    fn mirror_names(module: &Module) -> HashSet<String> {
        // The mirror emits `{prefix}.{var}`; with an empty prefix that is
        // `.{var}`, so strip the leading '.' (same normalization as
        // `module_bindings_fully_mirrorable`).
        let mut qualified = Vec::new();
        collect_assign_vars_in_stmts(&module.body.stmts, "", &mut qualified);
        qualified
            .iter()
            .map(|q| q.trim_start_matches('.').to_string())
            .collect()
    }

    /// Assert the EXACT per-shape classification of both collectors and the
    /// resulting mirrorability gate. A collector coverage change without a
    /// matching table update fails here (Issue #9996).
    #[test]
    fn mirror_coverage_table_matches_collectors_9996() {
        for row in coverage_table() {
            let module = module_with_body(row.stmts);
            let resolution = resolution_names(&module);
            let mirror = mirror_names(&module);
            assert_eq!(
                resolution.contains("x"),
                row.resolution_sees,
                "[{}] RESOLUTION collector (collect_module_body_binding_names) \
                 coverage changed — update the Issue #9996 table AND the ADR \
                 LV5 wording in the same change",
                row.shape
            );
            assert_eq!(
                mirror.contains("x"),
                row.mirror_sees,
                "[{}] STATE-MIRROR collector (collect_assign_vars_in_stmts) \
                 coverage changed — update the Issue #9996 table AND the ADR \
                 LV5 wording in the same change",
                row.shape
            );
            assert_eq!(
                module_bindings_fully_mirrorable(&module),
                row.live_path_eligible,
                "[{}] live/fallback classification changed — update the Issue \
                 #9996 table AND the ADR LV5 wording in the same change",
                row.shape
            );
            // The durable contract, per shape: mirror ⊇ resolution whenever the
            // shape is classified live-path eligible.
            if row.live_path_eligible {
                assert!(
                    resolution.iter().all(|name| mirror.contains(name)),
                    "[{}] classified LIVE but mirror ⊉ resolution: a live-mutated \
                     binding would be lost on a full-recompile fallback (#9989)",
                    row.shape
                );
            }
        }
    }

    /// The exact difference set `resolution \ mirror` — every construct where
    /// the RESOLUTION collector sees a binding but the STATE MIRROR does not
    /// (the Issue #9996 third checkbox: these are the only shapes allowed to be
    /// resolution-only, and they MUST classify as full-recompile fallback).
    /// Shrinking this set (e.g. teaching the mirror `AssignExpr`) or growing it
    /// requires editing this expectation explicitly.
    #[test]
    fn resolution_minus_mirror_difference_set_is_exact_9996() {
        let mut resolution_only: Vec<&'static str> = Vec::new();
        for row in coverage_table() {
            let module = module_with_body(row.stmts);
            let resolution = resolution_names(&module);
            let mirror = mirror_names(&module);
            if resolution.contains("x") && !mirror.contains("x") {
                resolution_only.push(row.shape);
                assert!(
                    !module_bindings_fully_mirrorable(&module),
                    "[{}] resolution-only bindings MUST fail closed to the full \
                     recompile (LV5b) — see Issue #9996",
                    row.shape
                );
            }
        }
        assert_eq!(
            resolution_only,
            vec!["AssignExpr", "empty LetBlock"],
            "the set of resolution-only (mirror-untracked) module binding shapes \
             drifted — update this expectation, the coverage table, and the ADR \
             LV5/LV5b wording together (Issue #9996)"
        );
    }
}

/// LV5b per-module-kind tests (Issue #9723): package `using X` modules,
/// submodules, typed modules (module-level struct / abstract type), and module
/// redefinition. Each test asserts the DURABLE INVARIANTS first (Issue #9996
/// conventions): the observed value sequence matches upstream goldens, and module
/// state is preserved across a live→full-recompile fallback. Path-policy
/// assertions (`took_live` / stayed full) are written only where the LV5b
/// eligibility gate (`session_modules_persistable` / `module_is_simple_persistable`)
/// explicitly requires that policy, and must be updated together with the gate
/// and the ADR LV5b wording.
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod lv5b_module_kinds_tests_9723 {
    use super::*;

    fn with_large_stack<F: FnOnce() + Send + 'static>(f: F) {
        std::thread::Builder::new()
            .stack_size(64 * 1024 * 1024)
            .spawn(f)
            .unwrap()
            .join()
            .unwrap()
    }

    fn i64_of(r: &REPLResult) -> Option<i64> {
        match &r.value {
            Some(Value::I64(v)) => Some(*v),
            _ => None,
        }
    }

    /// Run `seq`; every step must succeed. Returns the observed `i64` value per
    /// step and whether any step ran on the live path (vm-build 0).
    fn run_seq(seq: &[&str]) -> (Vec<Option<i64>>, bool) {
        let mut s = REPLSession::new(0);
        let mut vals = Vec::new();
        let mut took_live = false;
        for src in seq {
            let r = s.eval(src);
            assert!(r.success, "`{src}`: {:?}", r.error);
            vals.push(i64_of(&r));
            took_live |= s.last_vm_build_nanos() == Some(0);
        }
        (vals, took_live)
    }

    /// Assert the value sequence against upstream Julia goldens. Returns whether
    /// the live path ran anywhere for gate-anchored policy assertions.
    fn assert_models_equal(seq: &[&str], expected: &[Option<i64>]) -> bool {
        let (persistent_vals, persistent_took_live) = run_seq(seq);
        assert_eq!(
            persistent_vals, expected,
            "REPL diverged from the upstream-julia-authored expectation"
        );
        persistent_took_live
    }

    /// LV5b — SUBMODULES. A module with a submodule: references to both the
    /// parent surface (`Outer.record()`) and the nested surface
    /// (`Outer.Inner.bump()`, `Outer.Inner.xs`) match upstream goldens, and
    /// nested mutable const state survives a hard-scope `let` live delta.
    /// Expected values verified against upstream `julia` 1.12 (Issue #9723).
    #[test]
    fn submodule_state_legacy_equiv_across_live_and_fallback_9723() {
        with_large_stack(|| {
            // NOTE: the parent const is `rlog9723`, not `log` — a module const
            // whose name collides with a Base function mis-resolves inside module
            // function bodies (pre-existing full-path bug, Issue #10234).
            let seq = [
                "module Outer9723\n  const rlog9723 = Int[]\n  module Inner9723\n    const xs = Int[]\n    bump() = (push!(xs, 10); length(xs))\n  end\n  record() = (push!(rlog9723, 1); length(rlog9723))\nend",
                "Outer9723.Inner9723.bump()",
                "Outer9723.Inner9723.bump()",
                "Outer9723.record()",
                // Hard-scope `let`: executes in the same live transaction.
                "let q9723 = 5\n  q9723\nend",
                "Outer9723.Inner9723.bump()",
                "length(Outer9723.Inner9723.xs)",
            ];
            let expected = [None, Some(1), Some(2), Some(1), Some(5), Some(3), Some(3)];
            let took_live = assert_models_equal(&seq, &expected);
            // Path policy (anchored to `module_is_simple_persistable`): a
            // recursively-simple submodule-bearing module is live-path eligible
            // since Issue #9723.
            assert!(
                took_live,
                "a recursively-simple submodule module should be live-path eligible (#9723)"
            );
        });
    }

    /// LV5b — TYPED MODULES (module-level struct). References that construct
    /// the module struct (`M.Pt(3)`), call a module function returning it, and
    /// read a struct-array module const match upstream goldens, and the
    /// mutable typed state survives a full-recompile fallback. Expected values
    /// verified against upstream `julia` 1.12 (Issue #9723).
    #[test]
    fn typed_module_struct_state_legacy_equiv_across_live_and_fallback_9723() {
        with_large_stack(|| {
            let seq = [
                "module Typed9723\n  struct Pt\n    x::Int\n  end\n  const cache = Pt[]\n  make(v) = (p = Pt(v); push!(cache, p); p)\nend",
                "Typed9723.make(7).x",
                "Typed9723.make(8).x",
                "length(Typed9723.cache)",
                "Typed9723.Pt(3).x",
                "let q9723t = 5\n  q9723t\nend",
                "Typed9723.make(9).x",
                "length(Typed9723.cache)",
            ];
            let expected = [
                None,
                Some(7),
                Some(8),
                Some(2),
                Some(3),
                Some(5),
                Some(9),
                Some(3),
            ];
            let took_live = assert_models_equal(&seq, &expected);
            assert!(
                took_live,
                "a struct-bearing module should be live-path eligible (#9723)"
            );
        });
    }

    /// LV5b — TYPED MODULES (module-level abstract type). A module abstract
    /// type used for `isa` dispatch from a later reference eval stays correct.
    /// Expected values verified against upstream `julia`
    /// 1.12 (Issue #9723).
    #[test]
    fn typed_module_abstract_type_legacy_equiv_9723() {
        with_large_stack(|| {
            let seq = [
                "module Abs9723\n  abstract type Animal end\n  struct Dog <: Animal\n    n::Int\n  end\n  is_animal(x) = x isa Animal ? 1 : 0\nend",
                "Abs9723.is_animal(Abs9723.Dog(1))",
                "Abs9723.is_animal(42)",
                "Abs9723.Dog(7).n",
            ];
            let expected = [None, Some(1), Some(0), Some(7)];
            let took_live = assert_models_equal(&seq, &expected);
            assert!(
                took_live,
                "an abstract-type-bearing module should be live-path eligible (#9723)"
            );
        });
    }

    /// LV5b — CROSS-MODULE struct-valued submodule const (codex review of
    /// #9723): a submodule const holding PARENT-module struct instances must
    /// match upstream goldens, including across a live→full-recompile
    /// fallback where `restore_module_constants` reconstructs the struct
    /// values inside the SUBMODULE's scope. The module-restoration converter
    /// preserves each struct's qualified owner so the fallback cannot resolve a
    /// parent type as a nonexistent submodule-local binding. Expected values
    /// verified against upstream `julia` 1.12.
    #[test]
    fn cross_module_struct_valued_submodule_const_legacy_equiv_9723() {
        with_large_stack(|| {
            let seq = [
                "module XOuter9723\n  struct Pt\n    x::Int\n  end\n  module XInner9723\n    const cache = Any[]\n  end\n  add(v) = (push!(XInner9723.cache, Pt(v)); length(XInner9723.cache))\nend",
                "XOuter9723.add(1)",
                "let q9723x = 1\n  q9723x\nend",
                "XOuter9723.add(2)",
                "length(XOuter9723.XInner9723.cache)",
            ];
            let expected = [None, Some(1), Some(1), Some(2), Some(2)];
            assert_models_equal(&seq, &expected);
        });
    }

    /// LV5b — MODULE REDEFINITION. Upstream `julia` REPLACES the module binding
    /// on redefinition: the new module starts from ITS OWN initializers (state
    /// reset), and `__init__` re-fires. Verified upstream 1.12: bump→1,2,
    /// redefine, bump→1, length→1. The production model must match that (the
    /// redefinition eval always full-recompiles — `program.modules` non-empty —
    /// and later references re-enter the NEWLY parked VM).
    #[test]
    fn module_redefinition_resets_state_matching_upstream_9723() {
        with_large_stack(|| {
            let module_src =
                "module Rdef9723\n  const xs = Int[]\n  bump() = (push!(xs, 1); length(xs))\nend";
            let seq = [
                module_src,
                "Rdef9723.bump()",
                "Rdef9723.bump()",
                module_src,
                "Rdef9723.bump()",
                "length(Rdef9723.xs)",
            ];
            let expected = [None, Some(1), Some(2), None, Some(1), Some(1)];
            assert_models_equal(&seq, &expected);
        });
    }

    /// LV5b — PACKAGE `using X` MODULES stay FAIL-CLOSED (Issue #9723 keeps
    /// this deliberate): a package module's `__init__` establishes VM-local
    /// state that must re-run in each eval's fresh VM, so the session is never
    /// admitted to the live delta path — every eval after `using X`
    /// full-recompiles while matching upstream goldens.
    #[test]
    fn package_using_session_stays_fail_closed_9723() {
        with_large_stack(|| {
            let seq = ["using Printf", "x9723 = 1", "x9723 + 1"];
            let expected = [None, Some(1), Some(2)];
            let took_live = assert_models_equal(&seq, &expected);
            assert!(
                !took_live,
                "a package-using session must stay on the full recompile path \
                 (fail-closed, Issue #9723 — package __init__ re-run semantics)"
            );
        });
    }

    /// LV5b — modules that stay FAIL-CLOSED after #9723: inner `using`/`import`,
    /// module-level macros, and `baremodule` keep the full recompile path (the
    /// carried surface does not resolve them; `module_is_simple_persistable`
    /// rejects them), while remaining correct against upstream goldens.
    #[test]
    fn inner_using_module_stays_fail_closed_and_legacy_equiv_9723() {
        with_large_stack(|| {
            let seq = [
                "module IU9723\n  using Printf\n  fmt(v) = @sprintf(\"%d\", v)\n  n() = 41\nend",
                "IU9723.n() + 1",
            ];
            let expected = [None, Some(42)];
            let took_live = assert_models_equal(&seq, &expected);
            assert!(
                !took_live,
                "an inner-using module must stay on the full recompile path (LV5b fail-closed)"
            );
        });
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
fn with_large_stack<F: FnOnce() + Send + 'static>(f: F) {
    std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(f)
        .unwrap()
        .join()
        .unwrap();
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod documented_definition_echo_tests_10164 {
    //! Issue #10164: a docstring preceding a definition now lowers to a
    //! generated `__sjulia_doc_<Name>` main-block `Assign` (so Base and user
    //! docstrings register for `@doc`). The REPL's return-value logic must NOT
    //! count that generated statement as a user-typed value expression:
    //! `user_main_empty` (`session.rs`) is computed IGNORING doc-registration
    //! statements, so a documented definition-only input returns the same
    //! value as its undocumented form (`Nothing` for a bare `struct`/method,
    //! the assigned value for a `const`) instead of echoing the docstring.
    use super::*;

    /// A documented struct (definition-only) must return `Nothing`, exactly
    /// like the undocumented form — NOT echo its docstring. Before the
    /// `user_main_empty` fix, the generated `__sjulia_doc_S` assignment made
    /// `main` non-empty and the REPL echoed the docstring string.
    #[test]
    fn documented_definition_only_input_returns_nothing_not_docstring_10164() {
        with_large_stack(|| {
            // Documented struct: definition-only -> Nothing (no echo).
            let mut s = REPLSession::new(0);
            let doc_struct =
                s.eval("\"\"\"docstring for DocEchoS\"\"\"\nstruct DocEchoS\n    x::Int\nend");
            assert!(doc_struct.success, "eval failed: {:?}", doc_struct.error);
            assert!(
                doc_struct.value.is_none(),
                "a documented struct is definition-only and must not echo its \
                 docstring (Issue #10164); got value {:?}",
                doc_struct.value
            );

            // Parity with the undocumented struct (also Nothing).
            let mut s2 = REPLSession::new(0);
            let plain_struct = s2.eval("struct DocEchoSPlain\n    x::Int\nend");
            assert!(
                plain_struct.success,
                "eval failed: {:?}",
                plain_struct.error
            );
            assert!(
                plain_struct.value.is_none(),
                "undocumented struct baseline should return Nothing; got {:?}",
                plain_struct.value
            );

            // The docstring is still REGISTERED (the fix only suppresses the
            // echo, not the registration): `@doc` in the same eval retrieves it.
            let mut s3 = REPLSession::new(0);
            let doc_query = s3.eval(
                "\"\"\"docstring for DocEchoQ\"\"\"\nstruct DocEchoQ\n    x::Int\nend\nstring(@doc DocEchoQ)",
            );
            assert!(doc_query.success, "eval failed: {:?}", doc_query.error);
            match &doc_query.value {
                Some(Value::Str(text)) => assert!(
                    text.contains("docstring for DocEchoQ"),
                    "@doc should retrieve the registered docstring; got {text:?}"
                ),
                other => panic!("expected the registered docstring String, got {other:?}"),
            }
        });
    }

    /// A documented `const` assignment is a value-producing statement (upstream
    /// `const c = 5` echoes `5`), so it must echo the assigned value — the same
    /// as the undocumented form. The doc-registration statement must not change
    /// that (it is filtered out of `user_main_empty`, and the const assignment
    /// remains, keeping `main` non-empty).
    #[test]
    fn documented_const_echoes_assigned_value_like_undocumented_10164() {
        with_large_stack(|| {
            let mut s = REPLSession::new(0);
            let doc_const = s.eval("\"\"\"docstring for DocEchoC\"\"\"\nconst DocEchoC = 5");
            assert!(doc_const.success, "eval failed: {:?}", doc_const.error);
            assert!(
                matches!(doc_const.value, Some(Value::I64(5))),
                "a documented const must echo its assigned value (Issue #10164); got {:?}",
                doc_const.value
            );

            let mut s2 = REPLSession::new(0);
            let plain_const = s2.eval("const DocEchoCPlain = 5");
            assert!(plain_const.success, "eval failed: {:?}", plain_const.error);
            assert!(
                matches!(plain_const.value, Some(Value::I64(5))),
                "undocumented const baseline should echo 5; got {:?}",
                plain_const.value
            );
        });
    }
}

#[cfg(test)]
mod selective_import_persistence_tests_11176 {
    use super::*;

    #[test]
    fn same_module_selective_bindings_survive_later_eval_11176() {
        with_large_stack(|| {
            let mut session = REPLSession::new(0);
            let module =
                session.eval("module ReplImports11176\n  f11176() = 20\n  g11176() = 22\nend");
            assert!(module.success, "module eval failed: {:?}", module.error);

            let import = session.eval("import .ReplImports11176: f11176, g11176");
            assert!(import.success, "import eval failed: {:?}", import.error);

            let call = session.eval("f11176() + g11176()");
            assert!(call.success, "later eval failed: {:?}", call.error);
            assert!(
                matches!(call.value, Some(Value::I64(42))),
                "both same-module selective bindings must persist; got {:?}",
                call.value
            );
        });
    }
}

#[cfg(test)]
mod runtime_nominal_repl_internal_tests_11691 {
    use super::*;

    #[test]
    fn repl_rejected_runtime_enum_adoption_rolls_back_registry_11691() {
        with_large_stack(|| {
            let enum_name = "RejectedRuntimeEnum11691";
            assert!(!subset_julia_vm_bytecode::value::enum_registry::is_registered_enum(enum_name));
            let mut session = REPLSession::new(0);
            assert!(session.eval("0").success);
            let source =
                "if true\n@enum RejectedRuntimeEnum11691 rejected_runtime_member11691\nend";
            let mut parser = match Parser::new() {
                Ok(parser) => parser,
                Err(error) => unreachable!("parser creation failed: {error:?}"),
            };
            let outcome = match parser.parse(source) {
                Ok(outcome) => outcome,
                Err(error) => unreachable!("runtime enum parse failed: {error:?}"),
            };
            let mut lowering =
                Lowering::new_with_usings_and_macros(source, &session.usings, &session.macros);
            let context = crate::lowering::LambdaContext::for_repl_fragment(session.eval_count);
            let program = match lowering.lower_with_lambda_context(outcome, &context) {
                Ok(program) => program,
                Err(error) => unreachable!("runtime enum lowering failed: {error:?}"),
            };
            let prepared = match session.try_live_delta_run(&program, 1) {
                Some(prepared) => prepared,
                None => unreachable!("runtime enum live append was rejected"),
            };
            assert!(
                prepared.enum_registry_transaction.is_some(),
                "a runtime enum template must arm rollback before execution"
            );
            subset_julia_vm_bytecode::value::enum_registry::register_enum(
                enum_name,
                &[("rejected_runtime_member11691".to_string(), 0)],
            );
            assert!(subset_julia_vm_bytecode::value::enum_registry::is_registered_enum(enum_name));
            drop(prepared);
            assert!(
                !subset_julia_vm_bytecode::value::enum_registry::is_registered_enum(enum_name),
                "rejected runtime/compiler adoption leaked the enum registry mutation"
            );
        });
    }
}
