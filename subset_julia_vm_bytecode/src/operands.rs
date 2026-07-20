//! Bytecode instruction operand payloads.

use serde::{Deserialize, Serialize};

use subset_julia_vm_ir::Span;
use subset_julia_vm_types::types::TypeExpr;

use crate::BuiltinId;
use crate::{ArrayElementType, RuntimeNominalDefInfo};

/// Operands for a nominal definition that is allocated and published only when
/// execution reaches its top-level control-flow position (Issue #11654).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DefineRuntimeNominalOperands {
    /// Stable source identity within the compiled input; never a registry ID.
    pub site_id: u64,
    pub span: Span,
    pub definition: RuntimeNominalDefInfo,
    /// The current source fragment also contains a compatible root declaration
    /// for this binding. The VM may reuse that declaration's reserved registry
    /// identity instead of allocating a second nominal type (Issue #11684).
    #[serde(default)]
    pub coalesce_with_root: bool,
    /// Compiler-reserved concrete type used by an explicit inner
    /// constructor's `NewStruct(type_id, ..)` bytecode. The VM keeps exactly
    /// this row private until the runtime declaration marker is reached.
    #[serde(default)]
    pub reserved_struct_type_id: Option<usize>,
    /// Dormant inner-constructor function rows made visible together with the
    /// reserved struct. Constructor activation must not bind a Function value
    /// over the type's constant binding (Issue #11679).
    #[serde(default)]
    pub constructor_function_indices: Vec<usize>,
    /// Exact enum member prefix retained by catchable-error recovery.
    #[serde(default)]
    pub published_members: Option<Vec<String>>,
}

/// Literal array payloads that can be serialized without VM runtime values.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ArrayLiteralPayload {
    F64 { data: Vec<f64>, shape: Vec<usize> },
    I64 { data: Vec<i64>, shape: Vec<usize> },
    Bool { data: Vec<bool>, shape: Vec<usize> },
}

/// Operands for `Instr::PushModule`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleOperands {
    pub name: String,
    pub exports: Vec<String>,
    pub publics: Vec<String>,
    /// Whether unqualified exports from Base are visible in this module.
    /// Ordinary `module` declarations set this implicitly; `baremodule`
    /// declarations set it only after a non-selective `using Base` (#11410).
    #[serde(default = "module_base_exports_visible_default")]
    pub base_exports_visible: bool,
    /// Whether ordinary `module` syntax installed the implicit per-module
    /// `eval` and `include` bindings. A `baremodule` stays false even after an
    /// explicit non-selective `using Base` (Issue #11410).
    #[serde(default = "module_base_exports_visible_default")]
    pub implicit_standard_bindings: bool,
}

fn module_base_exports_visible_default() -> bool {
    true
}

/// Operands for the `CallTypedDispatchOrBuiltinStoreDict[Result]` variants:
/// `(builtin, function_name, arg_count, candidates, store_local)`.
/// `candidates` are candidate function indices.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypedDispatchStoreDict {
    pub builtin: BuiltinId,
    pub function_name: String,
    pub arg_count: usize,
    pub candidates: Vec<usize>,
    pub store_local: String,
}

/// One explicit `where` parameter binding for `Instr::CallStaticParametric`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StaticParamBinding {
    pub name: String,
    pub value: TypeExpr,
}

/// Runtime-validated fallback for an imprecisely selected static-parametric
/// constructor call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StaticParametricFallback {
    pub func_index: usize,
    pub bindings: Vec<StaticParamBinding>,
}

/// Operands for `Instr::CallStaticParametric`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StaticParametricCall {
    pub func_index: usize,
    pub arg_count: usize,
    pub bindings: Vec<StaticParamBinding>,
    /// Resolve `TypeVar` binding values through the caller frame. Most legacy
    /// static-parametric calls retain their existing literal binding behavior;
    /// constructor lowering opts in when forwarding an outer `where` binding.
    #[serde(default)]
    pub forward_caller_type_bindings: bool,
    /// Validate runtime arguments against the selected method after resolving
    /// explicit/forwarded `where` bindings. Constructor lowering enables this
    /// when imprecise static inference selects a sole explicit inner fallback.
    #[serde(default)]
    pub validate_argument_types: bool,
    /// A unique parameterized-self outer constructor to try when runtime
    /// validation rejects the primary explicit inner constructor.
    #[serde(default)]
    pub validation_fallback: Option<StaticParametricFallback>,
    /// `where`-binder names bound from *runtime* type-argument values pushed on
    /// the stack above the positional arguments, in declaration order — the
    /// `Foo{typeof(x)}(x)` / `Foo{t}(x)` form whose type arguments cannot be
    /// serialized as literal `TypeExpr` bindings (Issue #10998). The VM pops
    /// one value per name, converts it to a `JuliaType`, checks the binder's
    /// declared bounds (raising `MethodError` like upstream when they are
    /// violated), and installs it into the callee frame's type bindings.
    #[serde(default)]
    pub runtime_binding_names: Vec<String>,
}

/// One candidate row for `Instr::CallParametricConstructorDispatch`: an
/// explicit-parametric inner-constructor method paired with the `where`-binder
/// bindings its own self signature declares (Issue #10971). Binder names may
/// differ per candidate (`Foo{T}(x::Int) where T` vs `Foo{S}(x::String) where
/// S`), so each candidate carries its own binding set rather than sharing one
/// across the whole dispatch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParametricConstructorCandidate {
    pub func_index: usize,
    /// Compile-time-literal `where`-binder bindings for this candidate (the
    /// explicit type argument(s) were statically known, e.g. `Foo{Int}(x)`).
    #[serde(default)]
    pub bindings: Vec<StaticParamBinding>,
    /// This candidate's own binder names for the runtime type-argument
    /// VALUES pushed once above the positional arguments, in declaration
    /// order (generalizes `StaticParametricCall::runtime_binding_names`,
    /// Issue #10998, to a per-candidate binder name since each candidate's
    /// self signature may name its binder differently). Reserved for a
    /// future call site that carries a genuinely runtime type-argument value
    /// into this dispatch; the current emission sites always leave this
    /// empty because the explicit type argument(s) are compile-time literals.
    #[serde(default)]
    pub runtime_binding_names: Vec<String>,
}

/// Operands for `Instr::CallParametricConstructorDispatch`: runtime candidate
/// selection over an explicit-parametric constructor family by value
/// signature, then per-candidate `where`-binder installation into the
/// selected frame. Fixes Issue #10971 (`Foo{Int}(x)` where the explicit type
/// argument is a compile-time literal but `x` is runtime-unknown, so more
/// than one overloaded braced inner constructor stays compatible until the
/// runtime value is known): the type-argument values (if any) are pushed once
/// above the positional arguments; the positional arguments select the
/// candidate; the selected candidate's own binder names/bindings are then
/// installed into its frame. (Issue #10968 — the sibling case where the type
/// argument itself is a runtime `DataType` value — was found already fixed on
/// main by prior work and does not emit this instruction; see the
/// `struct_parametric_ctor_local_datatype_dispatch_10968` regression fixture.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParametricConstructorDispatchCall {
    /// Base struct name, used only for `MethodError` messages.
    pub base_name: String,
    pub arg_count: usize,
    /// Number of runtime type-argument `Value`s pushed on the stack above the
    /// positional arguments (0 when every candidate's binder is a
    /// compile-time literal in `bindings`).
    pub type_arg_value_count: usize,
    pub candidates: Vec<ParametricConstructorCandidate>,
}

/// Operands for `Instr::CallDynamic`.
///
/// `callee_name` is the compiler-resolved method-table identity, retained so
/// the runtime can build the same semantic `CallRequest` as callable-value
/// dispatch without guessing an identity from candidate order (Issue #10461).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DynamicCallOperands {
    pub callee_name: String,
    pub fallback_func_index: usize,
    pub arg_count: usize,
    pub candidates: Vec<DynamicCallCandidate>,
}

/// A runtime dispatch candidate for `Instr::CallDynamic` (Issue #6496).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DynamicCallCandidate {
    /// A real method candidate: global function index.
    Method(usize),
    /// A VM-native iterator type handled by the built-in collect boundary.
    NativeIterator(NativeIteratorKind),
}

/// The VM-native iterator families that `collect` dispatch must route to the
/// built-in representation boundary (Issue #6496).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NativeIteratorKind {
    Zip,
    Zip3,
    Zip4,
    Zip5,
    Zip6,
    Zip7,
    Generator,
}

impl NativeIteratorKind {
    /// The runtime type name this sentinel historically carried; still used
    /// for scoring against the actual argument's type name.
    pub fn type_name(self) -> &'static str {
        match self {
            NativeIteratorKind::Zip => "Zip",
            NativeIteratorKind::Zip3 => "Zip3",
            NativeIteratorKind::Zip4 => "Zip4",
            NativeIteratorKind::Zip5 => "Zip5",
            NativeIteratorKind::Zip6 => "Zip6",
            NativeIteratorKind::Zip7 => "Zip7",
            NativeIteratorKind::Generator => "Base.Generator",
        }
    }
}

/// Operands for `Instr::InvokeFunctionVariableWithKwargs`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvokeWithKwargs {
    pub arg_count: usize,
    pub declared_signature: Vec<String>,
    pub kwarg_names: Vec<String>,
    pub kwargs_splat_mask: Vec<bool>,
}

/// Operands for `Instr::CallFunctionVariableWithKwargsSplat`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallVarKwargsSplat {
    pub arg_count: usize,
    pub pos_splat_mask: Vec<bool>,
    pub kwarg_names: Vec<String>,
    pub kwargs_splat_mask: Vec<bool>,
}

/// Operands for slot-argument `CallSpecialize` variants.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallSpecializeSlots {
    pub spec_func_index: usize,
    pub slots: Vec<usize>,
}

/// Operands for direct call variants whose arguments are read from I64 slots.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallDirectSlots {
    pub func_index: usize,
    pub slots: Vec<usize>,
}

/// Operands for `Instr::PushResolvedFunction`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedFunctionOperands {
    pub name: String,
    pub candidate_indices: Vec<usize>,
}

/// Operands for `Instr::CreateResolvedClosure`. The candidate set freezes the
/// exact callable family at the closure's definition site; the capture names
/// are resolved from the executing lexical environment (Issue #9784).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedClosureOperands {
    pub name: String,
    pub capture_names: Vec<String>,
    pub candidate_indices: Vec<usize>,
}

/// Operands for `Instr::RegisterEnum` (Issue #5139).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegisterEnumOperands {
    pub type_name: String,
    /// `(member_name, value)` in declaration order.
    pub members: Vec<(String, i64)>,
    /// `None` publishes every source member binding. Recovered REPL enum
    /// definitions use `Some` to replay only constants whose store completed
    /// before an exception, while retaining the complete enum registry above.
    #[serde(default)]
    pub published_members: Option<Vec<String>>,
}

/// Bytecode-owned generator callable forms.
///
/// Runtime callables and type-object values stay VM-owned and are routed through
/// `MakeGeneratorRuntime`; serialized bytecode only stores stable function-table
/// references here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GeneratorCallableSpec {
    FunctionIndex(usize),
    FilteredFunctionIndex {
        map_func_index: usize,
        predicate_func_index: usize,
    },
    TupleSplatFunctionIndex(usize),
}

/// Operands for `Instr::MakeGenerator`: `(callable, result_element_type)`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MakeGeneratorOperands {
    pub callable: GeneratorCallableSpec,
    pub result_element_type: Option<ArrayElementType>,
}
