//! Type definitions for the VM.
//!
//! This module contains struct definitions used by the VM:
//! - `FunctionInfo`: Information about a compiled function
//! - `KwParamInfo`: Keyword parameter info
//! - `StructDefInfo`: Struct type definition
//! - `AbstractTypeDefInfo`: Abstract type definition
//! - `ShowMethodEntry`: Entry for Base.show method
//! - `SpecializationKey`, `SpecializedCode`, `SpecializableFunction`: Lazy AoT support
//! - `RuntimeCompileContext`: Context for runtime specialization
//! - `CompiledProgram`: A compiled program ready for execution

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::module_intern::ModuleInternTable;
use crate::shared_plan::SharedFunctionPlan;
use crate::struct_info::ParametricStructDef;
use subset_julia_vm_types::ir::core::Expr;

use crate::value::{Value, ValueType};
pub use crate::{
    AbstractTypeDefInfo, PrimitiveTypeDefInfo, ShowMethodEntry, SpecializableFunction,
    StructDefInfo,
};
use crate::{Instr, VarTypeTag};

fn default_method_min_world() -> u64 {
    1
}

/// Function information for the VM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionInfo {
    pub name: String,
    pub params: Vec<(String, ValueType)>,
    /// Keyword parameters with their default values
    pub kwparams: Vec<KwParamInfo>,
    pub entry: usize,
    pub return_type: ValueType,
    /// Original Julia return type when `return_type` would lose precision
    /// (e.g. `Union{Int64,String}` represented as `ValueType::Any`).
    #[serde(default)]
    pub return_julia_type: Option<subset_julia_vm_types::types::JuliaType>,
    /// True for methods written as `Base.f(...)` / `Base.:op(...)` extensions.
    #[serde(default)]
    pub is_base_extension: bool,
    /// True for methods lowered from `@generated` definitions. The VM uses this
    /// to route compatibility fallback bodies through the staged Expr cache
    /// instead of the direct-call fast path (Issue #5936).
    #[serde(default)]
    pub is_generated: bool,
    /// Provenance for compiler-synthesized lowering helpers. This is separate
    /// from the function spelling: a Julia user may legally define the same
    /// name/signature, while the helper remains a private callable identity
    /// rather than a generic method. It is serialized because cached programs
    /// build their public/private runtime indices from this bit (#9784).
    #[serde(default)]
    pub is_lowering_helper: bool,
    /// Monotonic source definition order from lowering. Reflection uses the
    /// same chronology as runtime method-table replacement even when REPL full
    /// rebuild merges current IR before older retained definitions (#9784).
    /// Zero is legacy/synthesized metadata and falls back to vector order.
    #[serde(default)]
    pub definition_order: u64,
    /// First runtime world where this method is visible to ordinary dispatch.
    #[serde(default = "default_method_min_world")]
    pub min_world: u64,
    /// Type parameters from where clause (for type binding support)
    pub type_params: Vec<subset_julia_vm_types::types::TypeParam>,
    /// Original JuliaType for each parameter (preserves parametric patterns like Complex{T})
    pub param_julia_types: Vec<subset_julia_vm_types::types::JuliaType>,
    /// Code boundary: start instruction index (inclusive)
    pub code_start: usize,
    /// Code boundary: end instruction index (exclusive)
    pub code_end: usize,
    /// Local slot names (index -> variable name)
    pub slot_names: Vec<String>,
    /// Statically known local slot storage tags (index -> type tag).
    #[serde(default)]
    pub slot_types: Vec<Option<VarTypeTag>>,
    /// Total number of local slots
    pub local_slot_count: usize,
    /// Slot indices for positional parameters (aligned with params)
    pub param_slots: Vec<usize>,
    /// Index of varargs parameter (if any). Varargs collects remaining args into a Tuple.
    /// For `function f(a, b, args...)`, vararg_param_index would be Some(2).
    pub vararg_param_index: Option<usize>,
    /// For Vararg{T, N}: fixed argument count N. None = any count. (Issue #2525)
    pub vararg_fixed_count: Option<usize>,
    /// Representative inline metadata from `@inline` / `@noinline` /
    /// `@propagate_inbounds` markers retained at the top of the function body.
    /// Mirrors upstream `CodeInfo.inlining`: 0 = default, 1 = inline,
    /// 2 = noinline (Issues #4977, #4980).
    #[serde(default)]
    pub inlining_meta: u8,
    /// Representative constant-propagation metadata from `Base.@constprop`
    /// markers. Mirrors upstream `Method.constprop` / `CodeInfo.constprop`:
    /// 0 = default, 1 = aggressive, 2 = none (Issues #4978, #4981).
    #[serde(default)]
    pub constprop_meta: u8,
    /// Representative `@nospecialize` bitmask retained from a statement-position
    /// `@nospecialize a b` marker. Mirrors upstream `Method.nospecialize`: bit
    /// `i` (0-based, over explicit positional parameters) is set when that
    /// parameter is nospecialized; a trailing `@specialize` clears the mask
    /// (Issue #4984).
    #[serde(default)]
    pub nospecialize_meta: i32,
    /// Representative `Base.@propagate_inbounds` metadata. Mirrors upstream
    /// `CodeInfo.propagate_inbounds` (Issue #4979).
    #[serde(default)]
    pub propagate_inbounds_meta: bool,
    /// Representative `Base.@nospecializeinfer` metadata. Mirrors upstream
    /// `CodeInfo.nospecializeinfer` (Issue #4979).
    #[serde(default)]
    pub nospecializeinfer_meta: bool,
    /// Representative `Base.@assume_effects` purity bitmask. Mirrors upstream
    /// `CodeInfo.purity` (`encode_effects_override` value): 0 = default
    /// (Issue #4983).
    #[serde(default)]
    pub purity_meta: u16,
    /// Name of the `where`-bound type parameter the body directly returns, when
    /// the method is of the shape `g(...) where {..., R, ...} = R` (the body is
    /// nothing but `return R`). Reflection inference uses this to bind `R` from
    /// the concrete call signature and recover a precise return type instead of
    /// widening to `Any` (Issue #4845).
    #[serde(default)]
    pub direct_return_type_param: Option<String>,
    /// 1-based source line of the function definition, taken from the IR
    /// `Function.span.start_line`. Surfaced as `Method.line` by `methods(f)`
    /// reflection so `show(::Method)` can render the ` @ Module file:line`
    /// suffix (Issue #5125). `0` when no source span is available (e.g. builtin
    /// stub `FunctionInfo`s synthesized in the VM).
    #[serde(default)]
    pub def_line: u32,
    /// True for a function collected directly from a module-body
    /// `let`/`@testset` (a lexically-scoped LOCAL of that block, not a
    /// genuine top-level module/global generic function — Issue #10236).
    ///
    /// Such a function's `name` is still module-qualified (`"Module.path.f"`)
    /// and still receives the compile-time bare-name `method_tables` alias
    /// (needed for the module's own in-scope calls to resolve it, Issue
    /// #7575's `module_owned_function_table_name` redirect). This flag ONLY
    /// suppresses the *runtime* `function_name_index` short-name alias that
    /// `VmState` derives from every dotted `FunctionInfo.name` — without it,
    /// two different modules' (or a module's and Main's) same-named
    /// `let`-root helper would both surface under the SAME bare runtime name,
    /// so a `Value::Closure`/`Value::Function` created for one scope's helper
    /// could resolve to the OTHER scope's body (`get_function_indices_by_name`
    /// has no type-based way to disambiguate two identical-arity untyped
    /// candidates).
    #[serde(default)]
    pub suppress_short_name_alias: bool,
    /// Runtime-only shared SSA plan used by the register VM gate (Issue #9089).
    ///
    /// It is not part of the Base/prelude cache wire format: cached methods
    /// stay on the stack VM under the register gate until they are compiled in
    /// this process and can hand the register backend the same planned IR the
    /// stack backend consumed.
    #[serde(skip)]
    pub shared_plan: Option<SharedFunctionPlan>,
}

/// Marker prefix for the synthesized `self` parameter of a bound-form
/// callable struct definition `(self::Type)(args)`.
///
/// Lowering (`parse_callable_self_param`) renames this leading parameter's
/// declared name to this marker plus the user's chosen identifier (`self`,
/// `callable`, ...), then re-introduces the plain identifier as an aliasing
/// local at the top of the body (`inject_callable_self_alias_prologue`) so
/// the body can still read it. The marker survives unchanged into
/// `FunctionInfo::params[0].0`, giving the runtime a ground-truth structural
/// signal for bound-ness — the parameter *name*, not its *type* — since type
/// alone cannot distinguish a genuine bound `self` from an anonymous-form
/// method whose own first parameter happens to be annotated with the
/// struct's own type (Issue #11553; the earlier arity-only heuristic broke
/// on vararg candidates for the same reason, Issue #11386).
pub const CALLABLE_SELF_BOUND_MARKER: &str = "__callable_self_bound__";

impl FunctionInfo {
    /// True when this method is a bound-form callable struct
    /// `(self::Type)(args)` — see `CALLABLE_SELF_BOUND_MARKER`.
    pub fn callable_binds_self(&self) -> bool {
        self.params
            .first()
            .is_some_and(|(name, _)| name.starts_with(CALLABLE_SELF_BOUND_MARKER))
    }
}

/// Stored signature-independent body for a method defined by runtime `eval`
/// of a quoted function-definition `Expr` (Issue #8647), keyed by the
/// `FunctionInfo`'s index in `Vm::functions`.
///
/// That `FunctionInfo`'s bytecode is a small fixed trampoline
/// (`CallBuiltin(EvalDefinedCall, 0); ReturnAny`) rather than a real
/// compiled body: SubsetJuliaVM has no JIT and no runtime Expr→bytecode
/// compiler, so an `eval`-defined method instead re-enters the tree-walking
/// `eval` interpreter over `body` on every call, resolving parameter
/// references by name against the values the normal call machinery already
/// bound into the frame's slots (`param_slots` is an identity mapping over
/// `slot_names`). This mirrors how `@generated` staged bodies are
/// interpreted rather than compiled (see `Vm::eval_generated_expr_value`).
///
/// Never part of `CompiledProgram` / the bincode wire format — this table
/// only exists in a running `Vm` and is populated purely at `eval` time.
#[derive(Debug, Clone)]
pub struct EvalDefinedMethod {
    /// The (unevaluated) function body, e.g. the `Expr(:block, ...)` from a
    /// short-form `f(x) = expr` definition.
    pub body: Value,
    /// The module the method was defined in, so name resolution inside the
    /// body matches the defining `eval`/`Module.eval` call rather than
    /// whatever module happens to be active on a later call.
    pub module_name: Option<String>,
}

/// Keyword parameter info for VM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KwParamInfo {
    pub name: String,
    pub default: Value,
    /// Original default expression for omitted keyword evaluation.
    #[serde(default)]
    pub default_expr: Option<Expr>,
    pub ty: ValueType,
    /// The keyword's DECLARED type (`x::Int64 = 1`), when annotated. Upstream
    /// treats a keyword annotation as an assertion on every supplied value
    /// (`TypeError: in keyword argument x, expected Int64, got a value of type
    /// Float64`), so the precise `JuliaType` must survive to the bind site —
    /// `ty` above is the lossy slot `ValueType`, which cannot express an
    /// abstract annotation such as `x::Real` (Issue #11024).
    #[serde(default)]
    pub declared_type: Option<subset_julia_vm_types::types::JuliaType>,
    pub slot: usize,
    /// True if this kwarg is required (no default value)
    pub required: bool,
    /// True if this is a varargs kwparam (kwargs...) that collects remaining kwargs
    pub is_varargs: bool,
}

// === Lazy AoT Compilation Support ===

/// Key for specialization cache lookup
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct SpecializationKey {
    pub func_index: usize,
    pub arg_types: Vec<ValueType>,
}

/// Specialized function code
#[derive(Debug, Clone)]
pub struct SpecializedCode {
    /// Entry point in the code vector
    pub entry: usize,
    /// Inferred return type for this specialization
    pub return_type: ValueType,
    /// Length of the specialized bytecode
    pub code_len: usize,
    /// Local slot count required by the specialized body.  Runtime
    /// specialization may introduce extra split slots (e.g. ComplexF64 SROA)
    /// that were not present in the fallback function, so the callee frame
    /// must be sized to this value rather than `fallback_func.local_slot_count`.
    pub local_slot_count: usize,
}

/// Pre-resolved direct-dispatch record for the all-`I64` specialize hot path
/// (Issue #8167).
///
/// `CallSpecializeI64Slots` originally rebuilt a `SpecializationKey {
/// func_index, arg_types: vec![I64; n] }` and hashed that `Vec`-keyed map on
/// *every* call, plus cloned the callee's `param_slots` `Vec` each time. For a
/// tight loop like `calc_pi`'s `mygcd` that is two heap allocations and a
/// `Vec`-key hash per call. Because the `I64Slots` instruction only fires when
/// every argument slot already holds an `I64`, the resolved specialization for a
/// given `(spec_func_index, arity)` is constant for the lifetime of the `Vm`, so
/// it can be resolved once and dispatched directly thereafter — the
/// "`CallResolvedI64Slots`-like direct call" described in #8159 proposal 1.
#[derive(Debug, Clone)]
pub struct I64SpecDispatch {
    /// Entry point of the specialized body in the code vector.
    pub entry: usize,
    /// One-past-the-end of the specialized body (`entry + code_len`).
    pub code_end: usize,
    /// Generic (fallback) function index, used for frame bookkeeping.
    pub fallback_index: usize,
    /// Local slot count of the callee frame.
    pub local_slot_count: usize,
    /// Parameter slot indices, shared (no per-call `Vec` clone).
    pub param_slots: std::rc::Rc<[usize]>,
}

/// Runtime compile context for specialization
#[derive(Debug, Clone)]
pub struct RuntimeCompileContext {
    /// Owner-scoped struct table (Issue #11078): entries are keyed by
    /// `StructId`, names are aliases into that id space. Never serialized
    /// (`CompiledProgram::compile_context` is `#[serde(skip)]`, Issue #3973) —
    /// it is rebuilt on both the fresh and cache-restore lanes, so the ids are
    /// DERIVED, not relocated (`docs/vm/CACHE_ARCHITECTURE.md` Pattern A).
    pub struct_table: crate::struct_registry::StructRegistry,
    pub struct_defs: Vec<StructDefInfo>,
    pub parametric_structs: HashMap<String, ParametricStructDef>,
    /// Top-level Base-origin parametric definitions keyed by bare family name.
    /// Kept separate from source-visible aliases so explicit `Base.T` lookup
    /// cannot change ordinary method-signature resolution (Issue #11369).
    pub base_parametric_structs: HashMap<String, ParametricStructDef>,
    pub type_aliases: HashMap<String, String>,
    /// Destination-qualified imported binding -> canonical source binding.
    /// Imports are live aliases, so runtime Module reflection follows this
    /// map instead of reading an assignment-time snapshot (Issue #11176).
    pub module_imported_bindings: HashMap<String, String>,
    /// Declared module path -> whether the module receives Base's complete
    /// export set (implicit for `module`, explicit for `baremodule`). Runtime
    /// reflection uses the declared owner rather than a reconstructed
    /// [`crate::value::ModuleValue`], which may carry no source provenance
    /// (Issue #11410).
    pub module_base_exports_visibility: HashMap<String, bool>,
    /// Declared module path -> whether ordinary `module` syntax installed the
    /// standard `eval`/`include` bindings. This differs from Base visibility
    /// for `baremodule; using Base` (Issue #11410).
    pub module_implicit_standard_bindings: HashMap<String, bool>,
    /// Canonical Base export names used to separate ordinary-module imports
    /// from internal Base functions and BuiltinId implementation entries.
    /// Rebuilt from the bundled Base source on cache restore (Issue #11410).
    pub base_exported_names: HashSet<String>,
    /// Top-level binding types prepared for runtime reflection: const bindings
    /// remain precise while ordinary globals are widened to `Any`.
    pub inference_global_types: HashMap<String, ValueType>,
    /// User-declared primitive types, so runtime type reflection can answer
    /// isprimitivetype / sizeof / supertype for them (Issue #5058).
    pub primitive_types: Vec<PrimitiveTypeDefInfo>,
    /// True when the program defines a user `getindex` override on a native
    /// array-like receiver (Issue #6657). The runtime function specializer then
    /// refuses to emit its native-indexing fast path for scalar `xs[i]`, so the
    /// generic body's runtime `getindex` dispatch (which can reach the override)
    /// is used instead. False in the common no-override case, leaving the hot
    /// indexing fast path untouched.
    pub disable_array_getindex_specialization: bool,
    /// True when the program defines a user `setindex!` override on a native
    /// array-like receiver (Issue #6806). The `IndexStore` native write fast path
    /// for a MemoryRef-backed `Array{T,N}` wrapper is then refused so the
    /// override is reached via `setindex!` dispatch. False in the common
    /// no-override case, leaving the hot write fast path untouched. Mirrors
    /// `disable_array_getindex_specialization` for the write side.
    pub disable_array_setindex_specialization: bool,
    /// True when the program defines a user `getproperty` override (Issue #8127).
    /// The function specializer then refuses to emit a direct `GetField` for
    /// `obj.field` reads, so the access goes through the interpreter's
    /// `getproperty` dispatch (which reaches the override). False in the common
    /// no-override case, leaving the hot struct-field fast path untouched.
    pub disable_field_access_specialization: bool,
    /// Module-path <-> [`crate::ModuleId`] interning table (Issue #10988 Phase
    /// 2a), built once by walking the program's module tree in registration
    /// order (`compile/collect.rs::register_module_ids`). Not serialized
    /// (`RuntimeCompileContext` as a whole is `#[serde(skip)]` on
    /// `CompiledProgram`, see Issue #3973) — it is rebuilt fresh from the
    /// (possibly cache-restored) module AST on every compile, so no
    /// persisted-id relocation is needed for this projection; contrast
    /// `CompiledProgram::module_registry`, which travels with the genuinely
    /// serialized `macro_bindings` table.
    pub module_registry: ModuleInternTable,
}

/// Finalized method-table decisions that keep runtime specialization from
/// bypassing user dispatch. Persisting the decision makes cache restore use
/// the same semantic authority as fresh compilation (Issue #10334).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpecializationDisableFlags {
    pub array_getindex: bool,
    pub array_setindex: bool,
    pub field_access: bool,
}

/// A compiled Julia program ready for execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompiledProgram {
    pub code: Vec<Instr>,
    /// Instruction-indexed source spans for user diagnostics. Empty for older
    /// caches or synthesized programs without source provenance.
    #[serde(default)]
    pub source_map: Vec<Option<subset_julia_vm_ir::Span>>,
    /// Per-function metadata. `Rc`-shared so the thread-local Base cache and
    /// every compile/VM built from it share the ~4969 Base `FunctionInfo`
    /// entries instead of deep-cloning them per compile (Issue #9140). The VM
    /// already consumes functions as `Rc<FunctionInfo>`; serde's `rc` feature
    /// serializes `Rc<T>` exactly like `T`, so the cache wire format is
    /// unchanged.
    pub functions: Vec<std::rc::Rc<FunctionInfo>>,
    pub struct_defs: Vec<StructDefInfo>,
    pub abstract_types: Vec<AbstractTypeDefInfo>,
    /// User-declared primitive types (`primitive type Name Bits end`, Issue #5058)
    #[serde(default)]
    pub primitive_types: Vec<PrimitiveTypeDefInfo>,
    /// Source-ordered `@enum` definitions. Publication remains bytecode-driven.
    #[serde(default)]
    pub enum_defs: Vec<crate::metadata::EnumDefInfo>,
    /// Registry of Base.show(io::IO, x::T) methods by type name
    pub show_methods: Vec<ShowMethodEntry>,
    /// Registry of Base.print(io::IO, x::T) methods by type name
    #[serde(default)]
    pub print_methods: Vec<ShowMethodEntry>,
    pub entry: usize,
    /// Functions that can be specialized at runtime (Lazy AoT)
    pub specializable_functions: Vec<SpecializableFunction>,
    /// Map from generic fallback function index to `specializable_functions`
    /// index for runtime `CallSpecialize` emission.
    ///
    /// This intentionally excludes reflection-only registrations, which may
    /// live in `specializable_functions` but must not bypass dispatch.
    #[serde(default)]
    pub runtime_specialization_map: Vec<(usize, usize)>,
    /// Deterministic snapshot of the top-level binding types used by runtime
    /// reflection. `compile_context` itself is transient, so cache restore must
    /// rehydrate this semantic state instead of defaulting every binding to
    /// `Any` (Issue #10333).
    ///
    /// Entries are sorted by binding name before persistence.
    #[serde(default)]
    pub inference_global_types_snapshot: Vec<(String, ValueType)>,
    /// Persisted specialization-safety decisions made from the resolved fresh
    /// method tables. Restore must replay these flags instead of re-deriving
    /// them from source spellings or a partial module walk (Issue #10334).
    #[serde(default)]
    pub specialization_disable_flags: SpecializationDisableFlags,
    /// Runtime compile context for specialization.
    ///
    /// This is reconstructed for serialized Base caches at load time. Keeping it
    /// out of bincode avoids making every nextest process deserialize the full
    /// prelude struct IR just to start a fixture. See Issue #3973.
    #[serde(skip)]
    pub compile_context: Option<RuntimeCompileContext>,
    /// Number of base functions (for REPL to track across evaluations)
    pub base_function_count: usize,
    /// Macro bindings visible per module, keyed by [`crate::ModuleId`]
    /// (Issue #10988 Phase 2a; previously keyed by the bare module-path
    /// `String` — `"Main"`, `"AbstractAlgebra"`, ...). Each value is the set
    /// of macro names (with the leading `@`) the module owns or sees via
    /// `using`. Backs function-form `isdefined(::Module, Symbol("@name"))`,
    /// which otherwise never consults the macro binding table (Issue #7948).
    /// Resolve a module-name string to its `ModuleId` via `module_registry`
    /// (serialized alongside this field, below) before indexing.
    #[serde(default)]
    pub macro_bindings:
        std::collections::HashMap<crate::ModuleId, std::collections::HashSet<String>>,
    /// Module-path <-> [`crate::ModuleId`] relocation table for
    /// `macro_bindings` (Issue #10988 Phase 2a cache-relocation pattern):
    /// serialized alongside the id-keyed table it resolves, so a cache
    /// restore recovers the exact same path -> id mapping a fresh compile of
    /// the same source produced, rather than re-deriving ids from scratch.
    /// `CACHE_VERSION` gates the wire-shape change (a pre-#10988 cache has no
    /// `ModuleId` keys and is invalidated on version mismatch before this
    /// field is ever read, per `docs/vm/CACHE_ARCHITECTURE.md`'s
    /// invalidate-on-mismatch contract — never a partial/best-effort decode).
    #[serde(default)]
    pub module_registry: ModuleInternTable,
    /// Global slot names (index -> variable name) for module/main scope
    pub global_slot_names: Vec<String>,
    /// Statically known global slot storage tags (index -> type tag).
    #[serde(default)]
    pub global_slot_types: Vec<Option<VarTypeTag>>,
    /// Total number of global slots
    pub global_slot_count: usize,
    /// Names still genuinely bound at module/main scope when the main block
    /// finished compiling — i.e. the compiler's `initialized_locals` snapshot
    /// taken at the end of `compile_main` (Issue #9157/#9182). A `let`/`@testset`
    /// block restores `initialized_locals` to its pre-block value on exit, so a
    /// brand-new local it introduces is ABSENT here even though its slot still
    /// exists in the compiled main bytecode (top-level `let` shares the main
    /// frame's slot numbering; no runtime instruction clears the written slot).
    /// The REPL session reads this to scope its `Vm::get_global`-based
    /// cross-eval persistence to real main-scope bindings only, so a `let`-local
    /// whose store happened to survive optimization does not leak into a later
    /// eval as a phantom global (Issue #9182).
    ///
    /// Compile-time-derived and specific to the user main block, so it is a
    /// runtime-only field: never serialized into the Base/prelude bytecode cache
    /// (`#[serde(skip)]`) and reconstructed fresh on every compile.
    #[serde(skip)]
    pub main_scope_names: std::collections::HashSet<String>,
}
