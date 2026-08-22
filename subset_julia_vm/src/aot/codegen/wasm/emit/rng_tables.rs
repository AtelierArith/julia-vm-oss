use crate::aot::{AotError, AotResult};
use subset_julia_vm_bytecode::rng::{FI, KI, WI};
use wasm_encoder::{ConstExpr, DataSection};

const TABLE_BYTES: usize = 256 * 8;

pub(super) struct RngTables {
    base: i32,
    data: Vec<u8>,
}

impl RngTables {
    pub(super) fn collect(base: i32) -> AotResult<Self> {
        let mut data = Vec::with_capacity(TABLE_BYTES * 3);
        for value in KI {
            data.extend_from_slice(&value.to_le_bytes());
        }
        for value in WI {
            data.extend_from_slice(&value.to_bits().to_le_bytes());
        }
        for value in FI {
            data.extend_from_slice(&value.to_bits().to_le_bytes());
        }
        Ok(Self { base, data })
    }

    pub(super) const fn ki(&self) -> i32 {
        self.base
    }

    pub(super) const fn wi(&self) -> i32 {
        self.base + TABLE_BYTES as i32
    }

    pub(super) const fn fi(&self) -> i32 {
        self.base + (TABLE_BYTES * 2) as i32
    }

    pub(super) fn end(&self) -> AotResult<i32> {
        let base = usize::try_from(self.base).map_err(|_| too_large())?;
        i32::try_from(base.checked_add(self.data.len()).ok_or_else(too_large)?)
            .map_err(|_| too_large())
    }

    pub(super) fn data_section(&self) -> DataSection {
        let mut section = DataSection::new();
        section.active(0, &ConstExpr::i32_const(self.base), self.data.clone());
        section
    }
}

fn too_large() -> AotError {
    super::super::types::unsupported(
        "Wasm normal RNG tables exceed the 32-bit generated-module address space",
    )
}
