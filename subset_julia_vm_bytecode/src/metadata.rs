//! Bytecode-owned program metadata that is serialized with compiled programs.

use serde::{Deserialize, Serialize};
use subset_julia_vm_types::{
    ir::core::StructDef,
    types::{JuliaType, TypeParam},
};

use crate::ValueType;

/// A source-ordered top-level definition activation emitted into REPL main.
///
/// The typed sequence is the transaction log used to validate the exact
/// interleaved prefix reached before a catchable error (Issue #9784).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ReplDefinitionActivation {
    Function(usize),
    /// One source method plus compiler-refreshed transitive callers published
    /// atomically at the same world (Issue #9784).
    FunctionGroup {
        primary: usize,
        refresh: Vec<usize>,
    },
    Struct(usize),
    AbstractType(usize),
    PrimitiveType(usize),
    Enum(usize),
    /// A runtime-conditional nominal definition that was actually reached and
    /// committed. Unlike the prefix variants above, this carries its observed
    /// registry identity and immutable source template (Issue #11654).
    RuntimeNominal(RuntimeNominalActivation),
}

/// Source-ordered `@enum` metadata persisted with a compiled REPL prefix.
///
/// The main bytecode still owns publication through `RegisterEnum`; this table
/// lets a later relocatable compile resolve an already-published enum without
/// replaying its source (Issues #9784/#11635).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnumDefInfo {
    pub name: String,
    pub base_type: String,
    pub members: Vec<(String, i64)>,
}

/// Struct type definition information.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StructDefInfo {
    pub name: String,
    pub is_mutable: bool,
    pub fields: Vec<(String, ValueType)>,
    #[serde(default)]
    pub field_julia_types: Vec<JuliaType>,
    /// Parent abstract type name (for `struct Dog <: Animal`)
    pub parent_type: Option<String>,
}

impl StructDefInfo {
    /// Check if this struct is isbits (immutable with all primitive fields).
    /// isbits types can be stored inline in arrays (AoS layout).
    pub fn is_isbits(&self) -> bool {
        self.is_isbits_with_struct_defs(&[])
    }

    pub fn is_isbits_with_struct_defs(&self, struct_defs: &[StructDefInfo]) -> bool {
        if self.is_mutable {
            return false;
        }
        if self.field_julia_types.len() == self.fields.len() {
            return self
                .field_julia_types
                .iter()
                .all(|field_type| julia_type_isbits(field_type, struct_defs));
        }
        self.fields
            .iter()
            .all(|(_, field_type)| value_type_isbits(field_type, struct_defs))
    }

    /// If this struct qualifies for the byte-contiguous `Float64` inline array
    /// layout (`ArrayElementType::StructInlineF64` / `ArrayData::StructF64`,
    /// Issue #9198 S4), return its field count; otherwise `None`.
    ///
    /// Qualification is purely structural (Design Principle 8/10 — no type-name
    /// special-casing): the struct must be an isbits *immutable* struct
    /// (`is_isbits_with_struct_defs`) with at least one field, and **every**
    /// field must be exactly `Float64`. This is the general form of the S2/S3
    /// SROA'd 2×f64 shape — it covers `Complex{Float64}` and a user
    /// `struct V2 x::Float64; y::Float64 end` alike, and any N-field all-`Float64`
    /// immutable struct. Mixed-type isbits structs (e.g. `Int64` + `Float64`)
    /// deliberately return `None` and stay on the existing boxed
    /// `StructInlineOf`/`StructOf` path; the fully general per-field byte-buffer
    /// layout is deferred within Issue #9198.
    pub fn inline_f64_field_count(&self, struct_defs: &[StructDefInfo]) -> Option<usize> {
        if self.is_mutable {
            return None;
        }
        if !self.is_isbits_with_struct_defs(struct_defs) {
            return None;
        }
        // Prefer the authoritative `field_julia_types` list; fall back to the
        // `fields` `ValueType`s when the Julia-type list is absent (older
        // metadata / cache shapes).
        if self.field_julia_types.len() == self.fields.len() && !self.field_julia_types.is_empty() {
            if self
                .field_julia_types
                .iter()
                .all(|t| t.name().as_ref() == "Float64")
            {
                return Some(self.field_julia_types.len());
            }
            return None;
        }
        if !self.fields.is_empty() && self.fields.iter().all(|(_, t)| matches!(t, ValueType::F64)) {
            return Some(self.fields.len());
        }
        None
    }

    /// The data size, in bytes, of an instance of this struct.
    pub fn layout_size_bytes(&self, struct_defs: &[StructDefInfo]) -> Option<usize> {
        let field_offsets = self.field_offsets_bytes(struct_defs)?;
        let mut offset = 0usize;
        let mut max_align = 1usize;
        if self.field_julia_types.len() == self.fields.len() {
            for (idx, field_type) in self.field_julia_types.iter().enumerate() {
                let (size, align) = julia_type_layout(field_type, struct_defs)?;
                max_align = max_align.max(align);
                offset = field_offsets.get(idx).copied()?.checked_add(size)?;
            }
            return Some(align_to(offset, max_align));
        }

        for (idx, (_, field_type)) in self.fields.iter().enumerate() {
            let (size, align) = value_type_layout(field_type, struct_defs)?;
            max_align = max_align.max(align);
            offset = field_offsets.get(idx).copied()?.checked_add(size)?;
        }
        Some(align_to(offset, max_align))
    }

    pub fn field_offsets_bytes(&self, struct_defs: &[StructDefInfo]) -> Option<Vec<usize>> {
        let mut offset = 0usize;
        let mut offsets = Vec::with_capacity(self.fields.len());
        if self.field_julia_types.len() == self.fields.len() {
            for field_type in &self.field_julia_types {
                let (size, align) = julia_type_layout(field_type, struct_defs)?;
                offset = align_to(offset, align);
                offsets.push(offset);
                offset = offset.checked_add(size)?;
            }
            return Some(offsets);
        }

        for (_, field_type) in &self.fields {
            let (size, align) = value_type_layout(field_type, struct_defs)?;
            offset = align_to(offset, align);
            offsets.push(offset);
            offset = offset.checked_add(size)?;
        }
        Some(offsets)
    }

    /// The alignment (in bytes) this struct requires when stored inline as a
    /// field of another struct or as an array element.
    pub fn layout_align_bytes(&self, struct_defs: &[StructDefInfo]) -> Option<usize> {
        let mut max_align = 1usize;
        if self.field_julia_types.len() == self.fields.len() {
            for field_type in &self.field_julia_types {
                let (_, align) = julia_type_layout(field_type, struct_defs)?;
                max_align = max_align.max(align);
            }
            return Some(max_align);
        }

        for (_, field_type) in &self.fields {
            let (_, align) = value_type_layout(field_type, struct_defs)?;
            max_align = max_align.max(align);
        }
        Some(max_align)
    }
}

fn align_to(offset: usize, align: usize) -> usize {
    if align <= 1 {
        offset
    } else {
        offset.div_ceil(align) * align
    }
}

fn value_type_isbits(field_type: &ValueType, struct_defs: &[StructDefInfo]) -> bool {
    match field_type {
        ValueType::Bool
        | ValueType::I8
        | ValueType::I16
        | ValueType::I32
        | ValueType::I64
        | ValueType::I128
        | ValueType::U8
        | ValueType::U16
        | ValueType::U32
        | ValueType::U64
        | ValueType::U128
        | ValueType::F16
        | ValueType::F32
        | ValueType::F64
        | ValueType::ComplexF32
        | ValueType::ComplexF64
        | ValueType::Char
        | ValueType::Nothing
        | ValueType::Missing => true,
        ValueType::Struct(type_id) => struct_defs
            .get(*type_id)
            .is_some_and(|def| def.is_isbits_with_struct_defs(struct_defs)),
        _ => false,
    }
}

fn julia_type_isbits(field_type: &JuliaType, struct_defs: &[StructDefInfo]) -> bool {
    match field_type.name().as_ref() {
        "Bool" | "Int8" | "Int16" | "Int32" | "Int64" | "Int128" | "UInt8" | "UInt16"
        | "UInt32" | "UInt64" | "UInt128" | "Float16" | "Float32" | "Float64" | "Char"
        | "Nothing" | "Missing" => true,
        name => struct_defs
            .iter()
            .find(|def| def.name == name)
            .is_some_and(|def| def.is_isbits_with_struct_defs(struct_defs)),
    }
}

fn value_type_layout(
    field_type: &ValueType,
    struct_defs: &[StructDefInfo],
) -> Option<(usize, usize)> {
    match field_type {
        ValueType::Bool | ValueType::I8 | ValueType::U8 => Some((1, 1)),
        ValueType::I16 | ValueType::U16 | ValueType::F16 => Some((2, 2)),
        ValueType::I32 | ValueType::U32 | ValueType::F32 | ValueType::Char => Some((4, 4)),
        ValueType::I64 | ValueType::U64 | ValueType::F64 => Some((8, 8)),
        ValueType::ComplexF32 => Some((8, 4)),
        ValueType::ComplexF64 => Some((16, 8)),
        ValueType::I128 | ValueType::U128 => Some((16, 16)),
        ValueType::Nothing | ValueType::Missing => Some((0, 1)),
        ValueType::Struct(type_id) => {
            let def = struct_defs.get(*type_id)?;
            if def.is_mutable {
                return Some((8, 8));
            }
            let size = def.layout_size_bytes(struct_defs)?;
            let align = def.layout_align_bytes(struct_defs)?;
            Some((size, align))
        }
        _ => Some((8, 8)),
    }
}

fn julia_type_layout(
    field_type: &JuliaType,
    struct_defs: &[StructDefInfo],
) -> Option<(usize, usize)> {
    match field_type.name().as_ref() {
        "Bool" | "Int8" | "UInt8" => Some((1, 1)),
        "Int16" | "UInt16" | "Float16" => Some((2, 2)),
        "Int32" | "UInt32" | "Float32" | "Char" => Some((4, 4)),
        "Int64" | "UInt64" | "Float64" => Some((8, 8)),
        "Int128" | "UInt128" => Some((16, 16)),
        "Nothing" | "Missing" => Some((0, 1)),
        name => {
            let Some(def) = struct_defs.iter().find(|def| def.name == name) else {
                return Some((8, 8));
            };
            if def.is_mutable {
                return Some((8, 8));
            }
            let size = def.layout_size_bytes(struct_defs)?;
            let align = def.layout_align_bytes(struct_defs)?;
            Some((size, align))
        }
    }
}

/// Abstract type definition information.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AbstractTypeDefInfo {
    pub name: String,
    /// Parent abstract type name (for `abstract type Mammal <: Animal`)
    pub parent: Option<String>,
    /// Type parameters for parametric abstract types.
    pub type_params: Vec<TypeParam>,
}

/// User-declared primitive type definition information.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrimitiveTypeDefInfo {
    pub name: String,
    /// Parent abstract type name (for `primitive type MyU8 <: Unsigned 8 end`).
    /// `None` defaults to `Any`.
    pub parent: Option<String>,
    /// Declared number of bits (always a positive multiple of 8).
    pub bits: u32,
}

/// Inert metadata for a nominal definition whose surrounding top-level
/// control flow decides whether it is installed (Issue #11654).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RuntimeNominalDefInfo {
    Struct(RuntimeStructDefInfo),
    AbstractType(AbstractTypeDefInfo),
    PrimitiveType(PrimitiveTypeDefInfo),
    Enum(EnumDefInfo),
}

/// Complete source definition plus the concrete runtime layout derived from it.
/// Keeping the source payload prevents type parameters and constructor ownership
/// from being silently erased at the compile/VM boundary (Issues #11678/#11679).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeStructDefInfo {
    pub source: Box<StructDef>,
    pub layout: StructDefInfo,
}

/// Authoritative VM-to-REPL record for one committed runtime-conditional
/// nominal definition (Issue #11654).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeNominalActivation {
    pub site_id: u64,
    pub span: subset_julia_vm_ir::Span,
    /// Index in the family-specific runtime registry.
    pub registry_id: usize,
    pub definition: RuntimeNominalDefInfo,
    /// This site reused a compatible root declaration from the same compiled
    /// input, so it did not append a new family-registry row (Issue #11684).
    #[serde(default)]
    pub coalesced_root: bool,
    /// For enums, the exact ordered member prefix published before completion
    /// or a catchable collision. `None` for the other nominal families.
    #[serde(default)]
    pub published_members: Option<Vec<String>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_nominal_metadata_round_trips_all_families_11654() {
        let definitions = vec![
            RuntimeNominalDefInfo::Struct(RuntimeStructDefInfo {
                source: Box::new(StructDef {
                    name: "RuntimeStruct11654".to_string(),
                    is_mutable: false,
                    is_base_origin: false,
                    type_params: Vec::new(),
                    parent_type: Some("Any".to_string()),
                    fields: vec![subset_julia_vm_types::ir::core::StructField {
                        name: "x".to_string(),
                        type_expr: Some(subset_julia_vm_types::types::TypeExpr::Concrete(
                            JuliaType::Int64,
                        )),
                        span: subset_julia_vm_ir::Span::new(0, 1, 1, 1, 1, 2),
                    }],
                    inner_constructors: Vec::new(),
                    global_new_helpers: Vec::new(),
                    span: subset_julia_vm_ir::Span::new(0, 1, 1, 1, 1, 2),
                }),
                layout: StructDefInfo {
                    name: "RuntimeStruct11654".to_string(),
                    is_mutable: false,
                    fields: vec![("x".to_string(), ValueType::I64)],
                    field_julia_types: vec![JuliaType::Int64],
                    parent_type: Some("Any".to_string()),
                },
            }),
            RuntimeNominalDefInfo::AbstractType(AbstractTypeDefInfo {
                name: "RuntimeAbstract11654".to_string(),
                parent: Some("Any".to_string()),
                type_params: vec![TypeParam::new("T".to_string())],
            }),
            RuntimeNominalDefInfo::PrimitiveType(PrimitiveTypeDefInfo {
                name: "RuntimePrimitive11654".to_string(),
                parent: Some("Any".to_string()),
                bits: 8,
            }),
            RuntimeNominalDefInfo::Enum(EnumDefInfo {
                name: "RuntimeEnum11654".to_string(),
                base_type: "Int32".to_string(),
                members: vec![("runtime_member_11654".to_string(), 0)],
            }),
        ];

        let bytes = bincode::serialize(&definitions);
        assert!(bytes.is_ok(), "serialize runtime nominal metadata");
        let restored: Result<Vec<RuntimeNominalDefInfo>, _> =
            bincode::deserialize(&bytes.unwrap_or_default());
        assert!(
            matches!(restored, Ok(restored) if restored == definitions),
            "deserialize runtime nominal metadata"
        );
    }

    #[test]
    fn abstract_type_metadata_round_trips_parameter_bounds_issue_10554() {
        let info = AbstractTypeDefInfo {
            name: "BoundedAbstract".to_string(),
            parent: Some("Any".to_string()),
            type_params: vec![
                TypeParam::with_upper_bound("T".to_string(), "Real".to_string()),
                TypeParam::with_lower_bound("U".to_string(), "Int64".to_string()),
            ],
        };

        let bytes = bincode::serialize(&info).expect("serialize abstract metadata");
        let decoded: AbstractTypeDefInfo =
            bincode::deserialize(&bytes).expect("deserialize abstract metadata");
        assert_eq!(decoded.name, info.name);
        assert_eq!(decoded.parent, info.parent);
        assert_eq!(decoded.type_params, info.type_params);
    }

    fn f64_struct(name: &str, n: usize) -> StructDefInfo {
        StructDefInfo {
            name: name.to_string(),
            is_mutable: false,
            fields: (0..n).map(|i| (format!("f{i}"), ValueType::F64)).collect(),
            field_julia_types: (0..n).map(|_| JuliaType::Float64).collect(),
            parent_type: None,
        }
    }

    #[test]
    fn inline_f64_field_count_accepts_all_float64_immutable_9198() {
        assert_eq!(f64_struct("V2", 2).inline_f64_field_count(&[]), Some(2));
        assert_eq!(f64_struct("V3", 3).inline_f64_field_count(&[]), Some(3));
        assert_eq!(f64_struct("V1", 1).inline_f64_field_count(&[]), Some(1));
    }

    #[test]
    fn inline_f64_field_count_rejects_non_qualifying_9198() {
        // Zero-field struct: no contiguous f64 layout.
        assert_eq!(f64_struct("Empty", 0).inline_f64_field_count(&[]), None);

        // Mutable struct is not isbits.
        let mut m = f64_struct("MV2", 2);
        m.is_mutable = true;
        assert_eq!(m.inline_f64_field_count(&[]), None);

        // Mixed Int64 + Float64 fields do not all-f64 qualify (stay boxed).
        let mixed = StructDefInfo {
            name: "Mixed".to_string(),
            is_mutable: false,
            fields: vec![
                ("a".to_string(), ValueType::I64),
                ("b".to_string(), ValueType::F64),
            ],
            field_julia_types: vec![JuliaType::Int64, JuliaType::Float64],
            parent_type: None,
        };
        assert_eq!(mixed.inline_f64_field_count(&[]), None);

        // All-Float32 (not Float64) does not qualify for the f64 layout.
        let f32s = StructDefInfo {
            name: "F32Pair".to_string(),
            is_mutable: false,
            fields: vec![
                ("x".to_string(), ValueType::F32),
                ("y".to_string(), ValueType::F32),
            ],
            field_julia_types: vec![JuliaType::Float32, JuliaType::Float32],
            parent_type: None,
        };
        assert_eq!(f32s.inline_f64_field_count(&[]), None);
    }
}

/// Entry for a registered Base.show method.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShowMethodEntry {
    /// The struct type name this show method handles.
    pub type_name: String,
    /// Function index in the functions table.
    pub func_index: usize,
}
