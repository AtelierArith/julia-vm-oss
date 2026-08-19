use crate::aot::types::StaticType;
use crate::aot::{AotError, AotResult, UnsupportedInstructionDiagnostic};
use wasm_encoder::ValType;

pub const ABI_VERSION: i32 = 2;
pub(super) const MAX_RANK: usize = 8;
pub(super) const MAX_DIMENSION: i64 = 1_i64 << 31;
pub(super) const DESCRIPTOR_FLAGS_OFFSET: u64 = 4;
pub(super) const DESCRIPTOR_ELEMENT_TAG_OFFSET: u64 = 8;
pub(super) const DESCRIPTOR_ELEMENT_SIZE_OFFSET: u64 = 12;
pub(super) const DESCRIPTOR_LAYOUT_OFFSET: u64 = 16;
pub(super) const DESCRIPTOR_RANK_OFFSET: u64 = 20;
pub(super) const DESCRIPTOR_DATA_PTR_OFFSET: u64 = 24;
pub(super) const DESCRIPTOR_RESERVED_OFFSET: u64 = 28;
pub(super) const DESCRIPTOR_ELEMENT_COUNT_OFFSET: u64 = 32;
pub(super) const DESCRIPTOR_HEADER_SIZE: i64 = 40;
pub(super) const DESCRIPTOR_AXIS_SIZE: i64 = 16;
pub(super) const DESCRIPTOR_DIM_OFFSET: u64 = 40;
pub(super) const DESCRIPTOR_STRIDE_OFFSET: u64 = 48;
pub(super) const FLAG_MODULE_OWNED: i32 = 1;
pub(super) const FLAG_READONLY: i32 = 2;
pub(super) const ALLOWED_FLAGS: i32 = FLAG_MODULE_OWNED | FLAG_READONLY;
pub const ELEMENT_TAG_UINT8: u32 = 1;
pub const ELEMENT_TAG_INT8: u32 = 2;
pub const ELEMENT_TAG_UINT16: u32 = 3;
pub const ELEMENT_TAG_INT16: u32 = 4;
pub const ELEMENT_TAG_UINT32: u32 = 5;
pub const ELEMENT_TAG_INT32: u32 = 6;
pub const ELEMENT_TAG_UINT64: u32 = 7;
pub const ELEMENT_TAG_INT64: u32 = 8;
pub const ELEMENT_TAG_FLOAT32: u32 = 9;
pub const ELEMENT_TAG_FLOAT64: u32 = 10;
pub const ELEMENT_TAG_BOOL: u32 = 11;

pub const ELEMENT_TAG_TABLE: [(&str, u32); 11] = [
    ("UInt8", ELEMENT_TAG_UINT8),
    ("Int8", ELEMENT_TAG_INT8),
    ("UInt16", ELEMENT_TAG_UINT16),
    ("Int16", ELEMENT_TAG_INT16),
    ("UInt32", ELEMENT_TAG_UINT32),
    ("Int32", ELEMENT_TAG_INT32),
    ("UInt64", ELEMENT_TAG_UINT64),
    ("Int64", ELEMENT_TAG_INT64),
    ("Float32", ELEMENT_TAG_FLOAT32),
    ("Float64", ELEMENT_TAG_FLOAT64),
    ("Bool", ELEMENT_TAG_BOOL),
];

#[derive(Clone, Copy)]
pub(super) struct DescriptorLayout {
    pub(super) element_tag: u32,
    pub(super) element_size: i32,
    pub(super) element_alignment: i64,
    pub(super) rank: usize,
}

pub(super) fn descriptor_layout(ty: &StaticType) -> AotResult<DescriptorLayout> {
    match ty {
        StaticType::Array {
            element,
            ndims: Some(rank),
        } if **element == StaticType::U8 && *rank <= MAX_RANK => Ok(DescriptorLayout {
            element_tag: ELEMENT_TAG_UINT8,
            element_size: 1,
            element_alignment: 1,
            rank: *rank,
        }),
        other => Err(unsupported(format!(
            "Wasm AoT cannot represent descriptor type `{}`",
            other.julia_type_name()
        ))),
    }
}

pub(super) fn value_type(ty: &StaticType) -> AotResult<Option<ValType>> {
    match ty {
        StaticType::I64 => Ok(Some(ValType::I64)),
        StaticType::F64 => Ok(Some(ValType::F64)),
        StaticType::I32 | StaticType::Bool | StaticType::U8 => Ok(Some(ValType::I32)),
        StaticType::Array {
            element,
            ndims: Some(rank),
        } if **element == StaticType::U8 && *rank <= MAX_RANK => Ok(Some(ValType::I32)),
        StaticType::Nothing => Ok(None),
        other => Err(unsupported(format!(
            "Wasm AoT cannot represent type `{}`",
            other.julia_type_name()
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::ELEMENT_TAG_TABLE;

    #[test]
    fn generated_wasm_element_tags_are_append_only_literals() {
        assert_eq!(
            ELEMENT_TAG_TABLE,
            [
                ("UInt8", 1),
                ("Int8", 2),
                ("UInt16", 3),
                ("Int16", 4),
                ("UInt32", 5),
                ("Int32", 6),
                ("UInt64", 7),
                ("Int64", 8),
                ("Float32", 9),
                ("Float64", 10),
                ("Bool", 11),
            ]
        );
    }
}

pub(super) fn unsupported(message: impl Into<String>) -> AotError {
    AotError::UnsupportedInstruction(
        UnsupportedInstructionDiagnostic::new(message).with_workaround(
            "use the Rust AoT backend or keep Wasm input within the documented static subset",
        ),
    )
}
