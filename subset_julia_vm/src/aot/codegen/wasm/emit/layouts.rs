use crate::aot::ir::{AggregateLayout, IrModule};
use crate::aot::{AotError, AotResult};
use wasm_encoder::{ConstExpr, DataSection};

use super::strings::STATIC_DATA_BASE;

pub(super) struct StaticLayouts {
    data: Vec<u8>,
    count: i32,
}

impl StaticLayouts {
    pub(super) fn collect(module: &IrModule) -> AotResult<Self> {
        let mut data = Vec::new();
        for layout in &module.layouts {
            write_layout(&mut data, layout)?;
        }
        Ok(Self {
            data,
            count: i32::try_from(module.layouts.len()).map_err(|_| too_large())?,
        })
    }

    pub(super) const fn table_address(&self) -> i32 {
        STATIC_DATA_BASE
    }

    pub(super) const fn count(&self) -> i32 {
        self.count
    }

    pub(super) fn end(&self) -> AotResult<i32> {
        let base = usize::try_from(STATIC_DATA_BASE).map_err(|_| too_large())?;
        i32::try_from(base.checked_add(self.data.len()).ok_or_else(too_large)?)
            .map_err(|_| too_large())
    }

    pub(super) fn append_data(&self, section: &mut DataSection) {
        if !self.data.is_empty() {
            section.active(
                0,
                &ConstExpr::i32_const(STATIC_DATA_BASE),
                self.data.clone(),
            );
        }
    }
}

fn write_layout(data: &mut Vec<u8>, layout: &AggregateLayout) -> AotResult<()> {
    let field_count = u32::try_from(layout.fields.len()).map_err(|_| too_large())?;
    data.extend_from_slice(&layout.id.to_le_bytes());
    data.extend_from_slice(&layout.size.to_le_bytes());
    data.extend_from_slice(&u32::from(layout.align).to_le_bytes());
    data.extend_from_slice(&field_count.to_le_bytes());
    for field in &layout.fields {
        data.extend_from_slice(&field.offset.to_le_bytes());
        data.extend_from_slice(&field_type_tag(&field.ty)?.to_le_bytes());
        data.extend_from_slice(&field.layout_id.to_le_bytes());
    }
    Ok(())
}

fn field_type_tag(ty: &crate::aot::types::StaticType) -> AotResult<u32> {
    use crate::aot::types::StaticType;
    Ok(match ty {
        StaticType::U8 => 1,
        StaticType::I8 => 2,
        StaticType::U16 => 3,
        StaticType::I16 => 4,
        StaticType::U32 => 5,
        StaticType::I32 => 6,
        StaticType::U64 => 7,
        StaticType::I64 => 8,
        StaticType::F32 => 9,
        StaticType::F64 => 10,
        StaticType::Bool => 11,
        StaticType::Tuple(_) | StaticType::NamedTuple(_) | StaticType::Struct { .. } => 0,
        other => {
            return Err(super::super::types::unsupported(format!(
                "Wasm layout metadata cannot encode `{}`",
                other.julia_type_name()
            )))
        }
    })
}

fn too_large() -> AotError {
    super::super::types::unsupported("Wasm aggregate layout table exceeds static memory")
}
