//! Compatibility re-exports for bytecode instructions.

pub use subset_julia_vm_bytecode::{
    ArrayLiteralPayload, CallDirectSlots, CallSpecializeSlots, CallVarKwargsSplat,
    DynamicCallCandidate, GeneratorCallableSpec, Instr, InvokeWithKwargs, MakeGeneratorOperands,
    ModuleOperands, NativeIteratorKind, RegisterEnumOperands, ResolvedFunctionOperands,
    StaticParamBinding, StaticParametricCall, StaticParametricFallback, TypedDispatchStoreDict,
};
