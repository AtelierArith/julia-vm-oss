//! Shared bytecode identifiers and program representation staging crate.
//!
//! Issue #8656 is splitting the compiler and VM into sibling crates. This crate
//! starts with the serialized bytecode identifiers whose declaration order is
//! part of Base cache compatibility.

pub mod array_element;
pub mod builtins;
pub mod effects;
pub mod enum_order;
pub mod error;
pub mod field_indices;
pub mod instr;
pub mod intrinsics;
pub mod metadata;
pub mod method_table;
pub mod module_intern;
pub mod operands;
pub mod peephole;
pub mod program;
pub mod rng;
pub mod runtime_constants;
pub mod runtime_signature;
pub mod runtime_types;
pub mod shared_plan;
pub mod simple_infer;
pub mod slot;
pub mod slot_metadata;
pub mod specialization;
pub mod stack_backend;
pub mod struct_info;
pub mod struct_registry;
pub mod type_intern;
pub mod type_name;
pub mod typed_scalar_ops;
pub mod value;
pub mod value_type;
pub mod wire_ids;

pub use array_element::ArrayElementType;
pub use array_element::{bare_type_name_array_element_type, builtin_type_array_element_type};
pub use builtins::{BuiltinBindingAuthority, BuiltinId};
pub use enum_order::julia_enum_member_binding_order;
pub use error::{ExceptionClass, VmError};
pub use instr::{I64Cmp, Instr, ScalarRandType};
pub use intrinsics::Intrinsic;
pub use metadata::{
    AbstractTypeDefInfo, EnumDefInfo, PrimitiveTypeDefInfo, ReplDefinitionActivation,
    RuntimeNominalActivation, RuntimeNominalDefInfo, RuntimeStructDefInfo, ShowMethodEntry,
    StructDefInfo,
};
pub use method_table::{MethodSig, MethodTable, ReplMethodIdentity};
pub use module_intern::{ModuleId, ModuleInternTable};
pub use operands::{
    ArrayLiteralPayload, CallDirectSlots, CallSpecializeSlots, CallVarKwargsSplat,
    DefineRuntimeNominalOperands, DynamicCallCandidate, DynamicCallOperands, GeneratorCallableSpec,
    InvokeWithKwargs, MakeGeneratorOperands, ModuleOperands, NativeIteratorKind,
    ParametricConstructorCandidate, ParametricConstructorDispatchCall, RegisterEnumOperands,
    ResolvedClosureOperands, ResolvedFunctionOperands, StaticParamBinding, StaticParametricCall,
    StaticParametricFallback, TypedDispatchStoreDict,
};
pub use program::{
    CompiledProgram, FunctionInfo, KwParamInfo, RuntimeCompileContext, SpecializationDisableFlags,
    SpecializationKey, SpecializedCode, CALLABLE_SELF_BOUND_MARKER,
};
pub use runtime_signature::{derived_runtime_signature, expanded_param_types_for_call};
pub use shared_plan::{
    SharedBlockPlan, SharedCopyPlan, SharedFunctionPlan, SharedRootPlan, SharedTermPlan,
};
pub use simple_infer::{
    infer_simple_function_return_type_for_value_args, promote_numeric_value_types,
};
pub use slot::{SlotInfo, SlotParamInfo};
pub use slot_metadata::VarTypeTag;
pub use specialization::SpecializableFunction;
pub use struct_info::{ParametricStructDef, StructInfo};
pub use struct_registry::{owner_module_path, StructId, StructRegistry};
pub use type_intern::{ConcreteTypeId, ConcreteTypeKey, TypeInternTable};
pub use type_name::parse_parametric_params;
pub use typed_scalar_ops::typed_scalar_binary_instr;
pub use value::SymbolValue;
pub use value::Value;
pub use value_type::ValueType;

#[cfg(test)]
mod tests {
    #[test]
    fn method_table_api_owned_by_bytecode_issue_9090() {
        fn assert_bytecode_owned<T>() {}

        assert_bytecode_owned::<crate::MethodSig>();
        assert_bytecode_owned::<crate::MethodTable>();
    }

    #[test]
    fn type_intern_api_owned_by_bytecode_issue_9090() {
        fn assert_bytecode_owned<T>() {}

        assert_bytecode_owned::<crate::ConcreteTypeId>();
        assert_bytecode_owned::<crate::ConcreteTypeKey>();
        assert_bytecode_owned::<crate::TypeInternTable>();
    }
}
