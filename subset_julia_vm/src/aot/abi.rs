//! Backend-neutral ABI classification for AoT values.
//!
//! The AoT IR should not let Rust-codegen details decide whether a value is a
//! native scalar, native aggregate/pointer, or runtime boxed `Value`.  This
//! module is the shared boundary that later Cranelift/native backends can use
//! without inheriting Rust emitter ad-hoc type decisions.

use crate::aot::types::StaticType;

/// ABI contract version expected by generated Rust when linking
/// `subset_julia_vm_runtime` (Issue #6952).
pub const AOT_RUNTIME_ABI_VERSION: usize = 1;

/// Backend-neutral representation class for an AoT value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AotAbiClass {
    /// Immediate scalar represented directly by the backend.
    UnboxedScalar,
    /// Native aggregate such as a tuple, range, or generated struct.
    NativeAggregate,
    /// Native pointer-like/owned heap value such as `String`, `Vec`, `HashMap`, or function pointer.
    NativePointer,
    /// Generic runtime value that must cross a boxed `Value` boundary.
    RuntimeBoxed,
}

/// ABI layout for one parameter, return value, local, field, or global.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AotAbiValue {
    julia_type: StaticType,
    class: AotAbiClass,
    rust_type: String,
}

impl AotAbiValue {
    /// Classify a static Julia type into the AoT ABI boundary.
    pub fn from_static_type(ty: &StaticType) -> Self {
        Self {
            julia_type: ty.clone(),
            class: Self::classify(ty),
            rust_type: ty.to_rust_type(),
        }
    }

    /// Original static Julia type used for inference and dispatch.
    pub fn julia_type(&self) -> &StaticType {
        &self.julia_type
    }

    /// ABI representation class.
    pub fn class(&self) -> AotAbiClass {
        self.class
    }

    /// Rust type spelling used by the existing Rust backend for this ABI value.
    pub fn rust_type(&self) -> &str {
        &self.rust_type
    }

    /// True when the value must cross the generic runtime `Value` boundary.
    pub fn needs_runtime_value(&self) -> bool {
        matches!(self.class, AotAbiClass::RuntimeBoxed)
    }

    /// True when the value can be passed without runtime boxing.
    pub fn is_native(&self) -> bool {
        !self.needs_runtime_value()
    }

    fn classify(ty: &StaticType) -> AotAbiClass {
        match ty {
            StaticType::I64
            | StaticType::I128
            | StaticType::I32
            | StaticType::I16
            | StaticType::I8
            | StaticType::U64
            | StaticType::U128
            | StaticType::U32
            | StaticType::U16
            | StaticType::U8
            | StaticType::F64
            | StaticType::F32
            | StaticType::F16
            | StaticType::Bool
            | StaticType::Char
            | StaticType::Nothing => AotAbiClass::UnboxedScalar,
            StaticType::Tuple(_)
            | StaticType::NamedTuple(_)
            | StaticType::Range { .. }
            | StaticType::Struct { .. } => AotAbiClass::NativeAggregate,
            StaticType::Str
            | StaticType::Array { .. }
            | StaticType::Dict { .. }
            | StaticType::Set { .. }
            | StaticType::Generator { .. }
            | StaticType::Function { .. } => AotAbiClass::NativePointer,
            StaticType::Union { variants } if variants.len() == 1 => Self::classify(&variants[0]),
            StaticType::Missing
            | StaticType::DataType
            | StaticType::Union { .. }
            | StaticType::Any => AotAbiClass::RuntimeBoxed,
        }
    }
}

/// ABI for a callable specialization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AotCallAbi {
    params: Vec<AotAbiValue>,
    ret: AotAbiValue,
}

impl AotCallAbi {
    /// Build a call ABI from inferred parameter and return types.
    pub fn from_signature(params: &[StaticType], ret: &StaticType) -> Self {
        Self {
            params: params.iter().map(AotAbiValue::from_static_type).collect(),
            ret: AotAbiValue::from_static_type(ret),
        }
    }

    /// Parameter ABI values in call order.
    pub fn params(&self) -> &[AotAbiValue] {
        &self.params
    }

    /// Return ABI value.
    pub fn ret(&self) -> &AotAbiValue {
        &self.ret
    }

    /// True when any argument or the return value needs the boxed runtime boundary.
    pub fn needs_runtime_value(&self) -> bool {
        self.params.iter().any(AotAbiValue::needs_runtime_value) || self.ret.needs_runtime_value()
    }

    /// True when all arguments and the return value can use native backend values.
    pub fn is_fully_native(&self) -> bool {
        !self.needs_runtime_value()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compiler_runtime_abi_version_matches_runtime_crate_issue_6952() {
        assert_eq!(
            AOT_RUNTIME_ABI_VERSION,
            subset_julia_vm_runtime::AOT_RUNTIME_ABI_VERSION
        );
    }

    #[test]
    fn primitive_scalars_are_unboxed() {
        let value = AotAbiValue::from_static_type(&StaticType::I64);

        assert_eq!(value.class(), AotAbiClass::UnboxedScalar);
        assert_eq!(value.rust_type(), "i64");
        assert!(value.is_native());
    }

    #[test]
    fn native_heap_values_are_pointer_like_not_runtime_boxed() {
        let value = AotAbiValue::from_static_type(&StaticType::Array {
            element: Box::new(StaticType::F64),
            ndims: Some(2),
        });

        assert_eq!(value.class(), AotAbiClass::NativePointer);
        assert_eq!(value.rust_type(), "Vec<Vec<f64>>");
        assert!(!value.needs_runtime_value());
    }

    #[test]
    fn native_structs_are_aggregates() {
        let value = AotAbiValue::from_static_type(&StaticType::Struct {
            type_id: 1,
            name: "Point".to_string(),
        });

        assert_eq!(value.class(), AotAbiClass::NativeAggregate);
        assert_eq!(value.rust_type(), "Point");
    }

    #[test]
    fn single_variant_union_uses_inner_class() {
        let value = AotAbiValue::from_static_type(&StaticType::Union {
            variants: vec![StaticType::I64],
        });

        assert_eq!(value.class(), AotAbiClass::UnboxedScalar);
        assert_eq!(value.rust_type(), "i64");
    }

    #[test]
    fn multi_variant_union_uses_runtime_value_boundary() {
        let value = AotAbiValue::from_static_type(&StaticType::Union {
            variants: vec![StaticType::I64, StaticType::F64],
        });

        assert_eq!(value.class(), AotAbiClass::RuntimeBoxed);
        assert_eq!(value.rust_type(), "Value");
        assert!(value.needs_runtime_value());
    }

    #[test]
    fn any_uses_runtime_value_boundary() {
        let value = AotAbiValue::from_static_type(&StaticType::Any);

        assert_eq!(value.class(), AotAbiClass::RuntimeBoxed);
        assert_eq!(value.rust_type(), "Value");
        assert!(!value.is_native());
    }

    #[test]
    fn call_abi_reports_runtime_value_use() {
        let abi =
            AotCallAbi::from_signature(&[StaticType::I64, StaticType::Any], &StaticType::Bool);

        assert_eq!(abi.params()[0].class(), AotAbiClass::UnboxedScalar);
        assert_eq!(abi.params()[1].class(), AotAbiClass::RuntimeBoxed);
        assert_eq!(abi.ret().class(), AotAbiClass::UnboxedScalar);
        assert!(abi.needs_runtime_value());
        assert!(!abi.is_fully_native());
    }

    #[test]
    fn call_abi_reports_fully_native_signature() {
        let abi = AotCallAbi::from_signature(&[StaticType::I64, StaticType::F64], &StaticType::F64);

        assert!(abi.is_fully_native());
        assert!(!abi.needs_runtime_value());
    }
}
