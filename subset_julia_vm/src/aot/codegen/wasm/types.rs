use crate::aot::types::StaticType;
use crate::aot::{AotError, AotResult, UnsupportedInstructionDiagnostic};
use wasm_encoder::ValType;

pub const ABI_VERSION: i32 = 1;
pub(super) const U8_ELEMENT_TYPE: i32 = 1;
pub(super) const DESCRIPTOR_PTR_OFFSET: u64 = 4;
pub(super) const DESCRIPTOR_LEN_OFFSET: u64 = 8;
pub(super) const DESCRIPTOR_ELEMENT_OFFSET: u64 = 12;
pub(super) const DESCRIPTOR_STRIDE_OFFSET: u64 = 16;

pub(super) fn value_type(ty: &StaticType) -> AotResult<Option<ValType>> {
    match ty {
        StaticType::I64 => Ok(Some(ValType::I64)),
        StaticType::F64 => Ok(Some(ValType::F64)),
        StaticType::I32 | StaticType::Bool | StaticType::U8 => Ok(Some(ValType::I32)),
        StaticType::Array {
            element,
            ndims: Some(1),
        } if **element == StaticType::U8 => Ok(Some(ValType::I32)),
        StaticType::Nothing => Ok(None),
        other => Err(unsupported(format!(
            "Wasm AoT cannot represent type `{}`",
            other.julia_type_name()
        ))),
    }
}

pub(super) fn unsupported(message: impl Into<String>) -> AotError {
    AotError::UnsupportedInstruction(
        UnsupportedInstructionDiagnostic::new(message).with_workaround(
            "use the Rust AoT backend or keep Wasm input within the documented static subset",
        ),
    )
}
