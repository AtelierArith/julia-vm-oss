use std::collections::{HashMap, HashSet};

use crate::aot::ir::{AggregateField, AggregateLayout, AotProgram, AotStruct};
use crate::aot::types::StaticType;
use crate::aot::AotResult;

use super::types::unsupported;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct FieldShape {
    size: u32,
    align: u8,
    layout_id: u32,
    primitive: Option<StaticType>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct LayoutShape(Vec<FieldShape>);

pub(super) struct LayoutRegistry {
    layouts: Vec<AggregateLayout>,
    shapes: HashMap<LayoutShape, u32>,
    types: HashMap<StaticType, u32>,
    structs: HashMap<String, AotStruct>,
    active: HashSet<String>,
}

impl LayoutRegistry {
    pub(super) fn collect(program: &AotProgram) -> AotResult<Self> {
        let mut registry = Self {
            layouts: Vec::new(),
            shapes: HashMap::new(),
            types: HashMap::new(),
            structs: program
                .structs
                .iter()
                .map(|definition| (definition.name.clone(), definition.clone()))
                .collect(),
            active: HashSet::new(),
        };
        for function in &program.functions {
            for (_, ty) in &function.params {
                registry.register_if_aggregate(ty)?;
            }
            registry.register_if_aggregate(&function.return_type)?;
        }
        Ok(registry)
    }

    pub(super) fn finish(self) -> Vec<AggregateLayout> {
        self.layouts
    }

    pub(super) fn register_if_aggregate(&mut self, ty: &StaticType) -> AotResult<()> {
        match ty {
            StaticType::Tuple(_) | StaticType::NamedTuple(_) | StaticType::Struct { .. } => {
                self.register(ty).map(|_| ())
            }
            _ => Ok(()),
        }
    }

    pub(super) fn id(&self, ty: &StaticType) -> AotResult<u32> {
        self.types.get(ty).copied().ok_or_else(|| {
            unsupported(format!(
                "Wasm aggregate layout is missing for `{}`",
                ty.julia_type_name()
            ))
        })
    }

    pub(super) fn layout(&self, ty: &StaticType) -> AotResult<&AggregateLayout> {
        let id = self.id(ty)?;
        self.layouts
            .get(usize::try_from(id - 1).map_err(|_| unsupported("invalid layout ID"))?)
            .ok_or_else(|| unsupported("invalid aggregate layout ID"))
    }

    pub(super) fn field(&self, ty: &StaticType, name: &str) -> AotResult<(u32, u32)> {
        let StaticType::Struct {
            name: type_name, ..
        } = ty
        else {
            return Err(unsupported(format!(
                "named field `{name}` requires a struct layout"
            )));
        };
        let definition = self
            .structs
            .get(type_name)
            .ok_or_else(|| unsupported(format!("unknown Wasm struct `{type_name}`")))?;
        let index = definition
            .fields
            .iter()
            .position(|(field, _)| field == name)
            .ok_or_else(|| unsupported(format!("unknown field `{name}` on `{type_name}`")))?;
        let layout = self.layout(ty)?;
        Ok((layout.id, layout.fields[index].offset))
    }

    pub(super) fn register(&mut self, ty: &StaticType) -> AotResult<u32> {
        if let Some(id) = self.types.get(ty) {
            return Ok(*id);
        }
        let active_name = match ty {
            StaticType::Struct { name, .. } => {
                if !self.active.insert(name.clone()) {
                    return Err(unsupported(format!("recursive isbits layout `{name}`")));
                }
                Some(name.clone())
            }
            _ => None,
        };
        let fields = self.field_types(ty)?;
        let mut aggregate_fields = Vec::with_capacity(fields.len());
        let mut shapes = Vec::with_capacity(fields.len());
        let mut size = 0_u32;
        let mut alignment = 1_u8;
        for field_ty in fields {
            let (field_size, field_align, nested_id, primitive) = self.field_shape(&field_ty)?;
            size = align_up(size, field_align)?;
            aggregate_fields.push(AggregateField {
                offset: size,
                ty: field_ty,
                layout_id: nested_id,
            });
            shapes.push(FieldShape {
                size: field_size,
                align: field_align,
                layout_id: nested_id,
                primitive,
            });
            size = size
                .checked_add(field_size)
                .ok_or_else(|| unsupported("aggregate layout size overflow"))?;
            alignment = alignment.max(field_align);
        }
        size = align_up(size, alignment)?;
        let shape = LayoutShape(shapes);
        let id = if let Some(id) = self.shapes.get(&shape) {
            *id
        } else {
            let id = u32::try_from(self.layouts.len() + 1)
                .map_err(|_| unsupported("too many aggregate layouts"))?;
            self.layouts.push(AggregateLayout {
                id,
                size,
                align: alignment,
                fields: aggregate_fields,
            });
            self.shapes.insert(shape, id);
            id
        };
        self.types.insert(ty.clone(), id);
        if let Some(name) = active_name {
            self.active.remove(&name);
        }
        Ok(id)
    }

    fn field_types(&mut self, ty: &StaticType) -> AotResult<Vec<StaticType>> {
        match ty {
            StaticType::Tuple(fields) => Ok(fields.clone()),
            StaticType::NamedTuple(fields) => Ok(fields.iter().map(|(_, ty)| ty.clone()).collect()),
            StaticType::Struct { name, .. } => {
                let definition = self
                    .structs
                    .get(name)
                    .cloned()
                    .ok_or_else(|| unsupported(format!("unknown Wasm struct `{name}`")))?;
                if definition.is_mutable || !definition.type_params.is_empty() {
                    return Err(unsupported(format!(
                        "Wasm aggregates require immutable non-parametric isbits struct `{name}`"
                    )));
                }
                Ok(definition.fields.into_iter().map(|(_, ty)| ty).collect())
            }
            _ => Err(unsupported(format!(
                "`{}` is not an aggregate layout",
                ty.julia_type_name()
            ))),
        }
    }

    fn field_shape(&mut self, ty: &StaticType) -> AotResult<(u32, u8, u32, Option<StaticType>)> {
        if let Some((size, align)) = primitive_layout(ty) {
            return Ok((size, align, 0, Some(ty.clone())));
        }
        match ty {
            StaticType::Tuple(_) | StaticType::NamedTuple(_) | StaticType::Struct { .. } => {
                let id = self.register(ty)?;
                Ok((4, 4, id, None))
            }
            _ => Err(unsupported(format!(
                "Wasm aggregate field `{}` is not isbits",
                ty.julia_type_name()
            ))),
        }
    }
}

fn primitive_layout(ty: &StaticType) -> Option<(u32, u8)> {
    match ty {
        StaticType::Bool | StaticType::I8 | StaticType::U8 => Some((1, 1)),
        StaticType::I16 | StaticType::U16 | StaticType::F16 => Some((2, 2)),
        StaticType::I32 | StaticType::U32 | StaticType::F32 | StaticType::Char => Some((4, 4)),
        StaticType::I64 | StaticType::U64 | StaticType::F64 => Some((8, 8)),
        StaticType::I128 | StaticType::U128 => Some((16, 16)),
        _ => None,
    }
}

fn align_up(value: u32, alignment: u8) -> AotResult<u32> {
    let mask = u32::from(alignment) - 1;
    value
        .checked_add(mask)
        .map(|sum| sum & !mask)
        .ok_or_else(|| unsupported("aggregate layout alignment overflow"))
}
