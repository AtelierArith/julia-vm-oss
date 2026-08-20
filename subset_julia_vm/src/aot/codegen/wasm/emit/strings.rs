use std::collections::HashMap;

use crate::aot::ir::{ConstValue, Instruction, IrModule};
use crate::aot::{AotError, AotResult};
use wasm_encoder::{ConstExpr, DataSection};

pub(super) const STATIC_DATA_BASE: i32 = 4_096;
const VIEW_SIZE: usize = 8;
const HEAP_ALIGNMENT: usize = 8;

pub(super) struct StaticStrings {
    offsets: HashMap<String, i32>,
    data: Vec<u8>,
    heap_base: i32,
}

impl StaticStrings {
    pub(super) fn collect(module: &IrModule) -> AotResult<Self> {
        let mut values = Vec::<&str>::new();
        let mut offsets = HashMap::new();
        for function in &module.functions {
            for block in &function.blocks {
                for instruction in &block.instructions {
                    let Instruction::LoadConst {
                        value: ConstValue::String(value),
                        ..
                    } = instruction
                    else {
                        continue;
                    };
                    if !offsets.contains_key(value) {
                        offsets.insert(value.clone(), 0);
                        values.push(value);
                    }
                }
            }
        }

        let descriptor_bytes = values.len().checked_mul(VIEW_SIZE).ok_or_else(too_large)?;
        let payload_bytes = values.iter().try_fold(0_usize, |total, value| {
            total.checked_add(value.len()).ok_or_else(too_large)
        })?;
        let total_bytes = descriptor_bytes
            .checked_add(payload_bytes)
            .ok_or_else(too_large)?;
        let mut data = vec![0_u8; total_bytes];
        let mut payload_offset = descriptor_bytes;
        for (index, value) in values.into_iter().enumerate() {
            let descriptor_offset = index.checked_mul(VIEW_SIZE).ok_or_else(too_large)?;
            let descriptor_address = address(descriptor_offset)?;
            let payload_address = address(payload_offset)?;
            let byte_len = u32::try_from(value.len()).map_err(|_| too_large())?;
            data[descriptor_offset..descriptor_offset + 4]
                .copy_from_slice(&payload_address.to_le_bytes());
            data[descriptor_offset + 4..descriptor_offset + VIEW_SIZE]
                .copy_from_slice(&byte_len.to_le_bytes());
            let payload_end = payload_offset
                .checked_add(value.len())
                .ok_or_else(too_large)?;
            data[payload_offset..payload_end].copy_from_slice(value.as_bytes());
            payload_offset = payload_end;
            offsets.insert(value.to_string(), descriptor_address);
        }
        let static_end = usize::try_from(STATIC_DATA_BASE)
            .map_err(|_| too_large())?
            .checked_add(total_bytes)
            .ok_or_else(too_large)?;
        let heap_base = align_heap(static_end)?;
        Ok(Self {
            offsets,
            data,
            heap_base,
        })
    }

    pub(super) fn descriptor(&self, value: &str) -> AotResult<i32> {
        self.offsets
            .get(value)
            .copied()
            .ok_or_else(|| AotError::InvalidIR("missing interned Wasm string literal".to_string()))
    }

    pub(super) const fn heap_base(&self) -> i32 {
        self.heap_base
    }

    pub(super) fn data_section(&self) -> Option<DataSection> {
        if self.data.is_empty() {
            return None;
        }
        let mut section = DataSection::new();
        section.active(
            0,
            &ConstExpr::i32_const(STATIC_DATA_BASE),
            self.data.clone(),
        );
        Some(section)
    }
}

fn address(offset: usize) -> AotResult<i32> {
    let base = usize::try_from(STATIC_DATA_BASE).map_err(|_| too_large())?;
    i32::try_from(base.checked_add(offset).ok_or_else(too_large)?).map_err(|_| too_large())
}

fn align_heap(value: usize) -> AotResult<i32> {
    let aligned = value
        .checked_add(HEAP_ALIGNMENT - 1)
        .ok_or_else(too_large)?
        & !(HEAP_ALIGNMENT - 1);
    i32::try_from(aligned).map_err(|_| too_large())
}

fn too_large() -> AotError {
    super::super::types::unsupported(
        "Wasm static UTF-8 literal data exceeds the 32-bit generated-module address space",
    )
}
