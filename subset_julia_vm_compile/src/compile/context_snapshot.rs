//! Deterministic semantic snapshots for cache-lane parity tests (Issue #10462).

// Whole-file test-only (declared `#[cfg(test)] pub(crate) mod context_snapshot;`
// in `compile/mod.rs`); this inner allow overrides that ancestor's
// `#![deny(clippy::unwrap_used)]`/`#![deny(clippy::expect_used)]` cascade
// (Issue #10908 Phase 3 of #10869).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;

use super::context::{SharedCompileContext, StructRegistry};
use super::{CResult, InstantiationKey, ParametricStructDef};
use crate::bytecode::{
    CompiledProgram, FunctionInfo, PrimitiveTypeDefInfo, RuntimeCompileContext,
    SpecializableFunction, StructDefInfo, StructInfo, ValueType,
};
use crate::ir::core::{Function, InnerConstructor, KwParam, StructDef, TypedParam};
use crate::types::{JuliaType, TypeExpr, TypeParam};
use serde::ser::{
    Error as _, SerializeMap, SerializeSeq, SerializeStruct, SerializeStructVariant,
    SerializeTuple, SerializeTupleStruct, SerializeTupleVariant,
};
use serde::Serialize;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CompileContextSnapshot {
    pub runtime_context_present: bool,
    pub semantic_structs: Vec<SemanticStructSnapshot>,
    pub struct_definitions: Vec<StructDefinitionSnapshot>,
    pub parametric_structs: Vec<ParametricStructSnapshot>,
    pub type_aliases: Vec<(String, String)>,
    pub inference_global_types: Vec<(String, ValueType)>,
    pub primitive_types: Vec<PrimitiveTypeSnapshot>,
    /// Registered module paths (Issue #10988 Phase 2a), sorted for
    /// deterministic comparison — NOT the `ModuleId` values themselves (those
    /// are dense allocation-order indices, meaningful only within one
    /// `ModuleInternTable`; comparing the raw ids across fresh/restored
    /// snapshots built independently would be comparing unrelated numbers).
    /// The path SET is what must match between lanes; `cache.rs`'s
    /// `assert_compile_context_parity` (Issue #10265) is where actual
    /// same-path-same-id parity is asserted, using both real
    /// `RuntimeCompileContext`s directly.
    pub module_registry: Vec<String>,
    pub main_scope_names: Vec<String>,
    pub method_signatures: Vec<MethodSignatureSnapshot>,
    pub promotion_registry: PromotionRegistrySnapshot,
    pub specialization_policy: SpecializationPolicySnapshot,
}

fn owner_test_parametric_def(name: &str, parent_type: &str) -> ParametricStructDef {
    ParametricStructDef {
        def: StructDef {
            name: name.to_string(),
            is_mutable: false,
            is_base_origin: false,
            type_params: vec![TypeParam::new("T".to_string())],
            parent_type: Some(parent_type.to_string()),
            fields: vec![],
            inner_constructors: vec![],
            global_new_helpers: vec![],
            span: crate::span::Span::new(0, 0, 0, 0, 0, 0),
        },
    }
}

fn owner_test_context(
    parametric_structs: HashMap<String, ParametricStructDef>,
) -> SharedCompileContext {
    SharedCompileContext::new(
        StructRegistry::new(),
        vec![],
        parametric_structs,
        HashMap::new(),
        vec![],
        0,
    )
}

#[test]
fn instantiation_recovers_unique_owner_from_colliding_bare_alias_issue_11264() -> CResult<()> {
    let family = "OwnerCollision11264";
    let base_def = owner_test_parametric_def(family, "Base.Number");
    let package_def = owner_test_parametric_def(family, "AbstractAlgebra.Ring");
    let mut ctx = owner_test_context(HashMap::from([
        (format!("Base.{family}"), base_def),
        (format!("AbstractAlgebra.{family}"), package_def.clone()),
        (family.to_string(), package_def),
    ]));

    let type_id = ctx.resolve_instantiation(family, &[JuliaType::BigInt])?;

    assert_eq!(
        ctx.get_struct_name(type_id).as_deref(),
        Some("AbstractAlgebra.OwnerCollision11264{BigInt}")
    );
    assert!(ctx.instantiation_table.contains_key(&InstantiationKey {
        base_name: "AbstractAlgebra.OwnerCollision11264".to_string(),
        type_args: vec![TypeExpr::Concrete(JuliaType::BigInt)],
    }));
    Ok(())
}

#[test]
fn instantiation_uses_registered_collision_before_other_owner_is_in_context_11264() -> CResult<()> {
    let family = "TemporalCollision11264";
    crate::types::register_type_name(&format!("Base.{family}"));
    crate::types::register_type_name(&format!("AbstractAlgebra.{family}"));
    let package_def = owner_test_parametric_def(family, "AbstractAlgebra.Ring");
    let mut ctx = owner_test_context(HashMap::from([
        (format!("AbstractAlgebra.{family}"), package_def.clone()),
        (family.to_string(), package_def),
    ]));

    let type_id = ctx.resolve_instantiation(family, &[JuliaType::BigInt])?;

    assert_eq!(
        ctx.get_struct_name(type_id).as_deref(),
        Some("AbstractAlgebra.TemporalCollision11264{BigInt}")
    );
    Ok(())
}

#[test]
fn instantiation_keeps_bare_name_without_owner_collision_issue_11264() -> CResult<()> {
    let family = "NoCollision11264";
    let package_def = owner_test_parametric_def(family, "OnlyModule.Ring");
    let mut ctx = owner_test_context(HashMap::from([
        (format!("OnlyModule.{family}"), package_def.clone()),
        (family.to_string(), package_def),
    ]));

    let type_id = ctx.resolve_instantiation(family, &[JuliaType::BigInt])?;

    assert_eq!(
        ctx.get_struct_name(type_id).as_deref(),
        Some("NoCollision11264{BigInt}")
    );
    assert_eq!(
        ctx.struct_table
            .resolve_in_owner("OnlyModule", "NoCollision11264{BigInt}")
            .map(|(_, info)| info.type_id),
        Some(type_id),
        "a bare display spelling must not erase the declaring module owner",
    );
    Ok(())
}

#[test]
fn explicit_base_instantiation_reuses_bare_concrete_identity_11369() -> CResult<()> {
    let family = "BaseIdentity11369";
    let mut base_def = owner_test_parametric_def(family, "Base.Any");
    base_def.def.is_base_origin = true;
    let mut ctx = SharedCompileContext::new(
        StructRegistry::new(),
        vec![],
        HashMap::from([(family.to_string(), base_def.clone())]),
        HashMap::from([(family.to_string(), base_def)]),
        vec![],
        0,
    );

    let bare_type_id = ctx.resolve_instantiation(family, &[JuliaType::Int64])?;
    let explicit_type_id =
        ctx.resolve_instantiation(&format!("Base.{family}"), &[JuliaType::Int64])?;

    assert_eq!(explicit_type_id, bare_type_id);
    assert_eq!(
        ctx.get_struct_name(explicit_type_id).as_deref(),
        Some("BaseIdentity11369{Int64}")
    );
    Ok(())
}

#[test]
fn exact_struct_lookup_does_not_select_parametric_family_instantiation_issue_11264() {
    let mut ctx = owner_test_context(HashMap::new());
    ctx.struct_table.insert(
        "SubArray{Int64,1}",
        StructInfo {
            type_id: 0,
            is_mutable: false,
            fields: vec![],
            has_inner_constructor: false,
        },
    );
    ctx.struct_defs.push(StructDefInfo {
        name: "SubArray{Int64,1}".to_string(),
        is_mutable: false,
        fields: vec![],
        field_julia_types: vec![],
        parent_type: None,
    });
    ctx.struct_name_to_def_index
        .insert("SubArray{Int64,1}".to_string(), 0);

    assert_eq!(ctx.get_exact_struct_type_id("SubArray"), None);
    assert_eq!(ctx.get_exact_struct_type_id("SubArray{Int64,1}"), Some(0));
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TypeParamSnapshot {
    pub name: String,
    pub upper_bound: Option<String>,
    pub lower_bound: Option<String>,
    pub legacy_bound: Option<String>,
}

impl From<&TypeParam> for TypeParamSnapshot {
    fn from(param: &TypeParam) -> Self {
        let TypeParam {
            name,
            upper_bound,
            lower_bound,
            bound,
        } = param;
        Self {
            name: name.clone(),
            upper_bound: upper_bound.clone(),
            lower_bound: lower_bound.clone(),
            legacy_bound: bound.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SemanticStructSnapshot {
    pub binding: String,
    pub type_id: usize,
    pub definition_name: Option<String>,
    pub is_mutable: bool,
    pub fields: Vec<(String, ValueType)>,
    pub has_inner_constructor: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StructDefinitionSnapshot {
    pub type_id: usize,
    pub name: String,
    pub is_mutable: bool,
    pub fields: Vec<(String, ValueType)>,
    pub field_julia_types: Vec<JuliaType>,
    pub parent_type: Option<String>,
}

impl StructDefinitionSnapshot {
    fn capture(type_id: usize, definition: &StructDefInfo) -> Self {
        let StructDefInfo {
            name,
            is_mutable,
            fields,
            field_julia_types,
            parent_type,
        } = definition;
        Self {
            type_id,
            name: name.clone(),
            is_mutable: *is_mutable,
            fields: fields.clone(),
            field_julia_types: field_julia_types.clone(),
            parent_type: parent_type.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TypedParamSnapshot {
    pub name: String,
    pub type_annotation: Option<JuliaType>,
    pub is_varargs: bool,
    pub vararg_count: Option<usize>,
}

impl From<&TypedParam> for TypedParamSnapshot {
    fn from(param: &TypedParam) -> Self {
        let TypedParam {
            name,
            type_annotation,
            is_varargs,
            vararg_count,
            span: _,
        } = param;
        Self {
            name: name.clone(),
            type_annotation: type_annotation.clone(),
            is_varargs: *is_varargs,
            vararg_count: *vararg_count,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct KwParamSnapshot {
    pub name: String,
    pub type_annotation: Option<JuliaType>,
    pub is_varargs: bool,
    pub body_evaluated_default: bool,
}

impl From<&KwParam> for KwParamSnapshot {
    fn from(param: &KwParam) -> Self {
        let KwParam {
            name,
            default: _,
            type_annotation,
            is_varargs,
            body_evaluated_default,
            span: _,
        } = param;
        Self {
            name: name.clone(),
            type_annotation: type_annotation.clone(),
            is_varargs: *is_varargs,
            body_evaluated_default: *body_evaluated_default,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InnerConstructorSignatureSnapshot {
    pub params: Vec<TypedParamSnapshot>,
    pub kwparams: Vec<KwParamSnapshot>,
    pub type_params: Vec<TypeParamSnapshot>,
    pub is_explicit_parametric: bool,
    pub explicit_type_parameter_names: Vec<String>,
    pub explicit_type_arguments: Vec<TypeExpr>,
}

impl From<&InnerConstructor> for InnerConstructorSignatureSnapshot {
    fn from(constructor: &InnerConstructor) -> Self {
        let InnerConstructor {
            params,
            kwparams,
            type_params,
            is_explicit_parametric,
            explicit_type_parameter_names,
            explicit_type_arguments,
            body: _,
            span: _,
        } = constructor;
        Self {
            params: params.iter().map(Into::into).collect(),
            kwparams: kwparams.iter().map(Into::into).collect(),
            type_params: type_params.iter().map(Into::into).collect(),
            is_explicit_parametric: *is_explicit_parametric,
            explicit_type_parameter_names: explicit_type_parameter_names.clone(),
            explicit_type_arguments: explicit_type_arguments.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ParametricStructSnapshot {
    pub binding: String,
    pub definition_name: String,
    pub is_mutable: bool,
    pub is_base_origin: bool,
    pub type_params: Vec<TypeParamSnapshot>,
    pub parent_type: Option<String>,
    pub fields: Vec<(String, Option<TypeExpr>)>,
    pub inner_constructors: Vec<InnerConstructorSignatureSnapshot>,
}

impl ParametricStructSnapshot {
    fn capture(binding: &str, definition: &StructDef) -> Self {
        let StructDef {
            name,
            is_mutable,
            is_base_origin,
            type_params,
            parent_type,
            fields,
            inner_constructors,
            // Lowering moves struct-body `global` helpers into the program's
            // function list, so a StructDef reaching the compiler always has an
            // empty list here — nothing to snapshot (Issue #11005).
            global_new_helpers: _,
            span: _,
        } = definition;
        Self {
            binding: binding.to_string(),
            definition_name: name.clone(),
            is_mutable: *is_mutable,
            is_base_origin: *is_base_origin,
            type_params: type_params.iter().map(Into::into).collect(),
            parent_type: parent_type.clone(),
            fields: fields
                .iter()
                .map(|field| {
                    let crate::ir::core::StructField {
                        name,
                        type_expr,
                        span: _,
                    } = field;
                    (name.clone(), type_expr.clone())
                })
                .collect(),
            inner_constructors: inner_constructors.iter().map(Into::into).collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PrimitiveTypeSnapshot {
    pub index: usize,
    pub name: String,
    pub parent: Option<String>,
    pub bits: u32,
}

impl PrimitiveTypeSnapshot {
    fn capture(index: usize, primitive: &PrimitiveTypeDefInfo) -> Self {
        let PrimitiveTypeDefInfo { name, parent, bits } = primitive;
        Self {
            index,
            name: name.clone(),
            parent: parent.clone(),
            bits: *bits,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RuntimeKwSignatureSnapshot {
    pub name: String,
    pub ty: ValueType,
    pub required: bool,
    pub is_varargs: bool,
    pub has_runtime_default_expr: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MethodSignatureSnapshot {
    pub function_index: usize,
    pub name: String,
    pub params: Vec<(String, ValueType)>,
    pub kwparams: Vec<RuntimeKwSignatureSnapshot>,
    pub param_julia_types: Vec<JuliaType>,
    pub type_params: Vec<TypeParamSnapshot>,
    pub return_type: ValueType,
    pub return_julia_type: Option<JuliaType>,
    pub vararg_param_index: Option<usize>,
    pub vararg_fixed_count: Option<usize>,
    pub min_world: u64,
    pub is_base_extension: bool,
    pub is_generated: bool,
    pub is_lowering_helper: bool,
    pub definition_order: u64,
    pub inlining_meta: u8,
    pub constprop_meta: u8,
    pub nospecialize_meta: i32,
    pub propagate_inbounds_meta: bool,
    pub nospecializeinfer_meta: bool,
    pub purity_meta: u16,
    pub direct_return_type_param: Option<String>,
    pub def_line: u32,
    pub suppress_short_name_alias: bool,
}

impl MethodSignatureSnapshot {
    fn capture(function_index: usize, function: &FunctionInfo) -> Self {
        let FunctionInfo {
            name,
            params,
            kwparams,
            entry: _,
            return_type,
            return_julia_type,
            is_base_extension,
            is_generated,
            is_lowering_helper,
            definition_order,
            min_world,
            type_params,
            param_julia_types,
            code_start: _,
            code_end: _,
            slot_names: _,
            slot_types: _,
            local_slot_count: _,
            param_slots: _,
            vararg_param_index,
            vararg_fixed_count,
            inlining_meta,
            constprop_meta,
            nospecialize_meta,
            propagate_inbounds_meta,
            nospecializeinfer_meta,
            purity_meta,
            direct_return_type_param,
            def_line,
            suppress_short_name_alias,
            shared_plan: _,
        } = function;
        Self {
            function_index,
            name: name.clone(),
            params: params.clone(),
            kwparams: kwparams
                .iter()
                .map(|param| RuntimeKwSignatureSnapshot {
                    name: param.name.clone(),
                    ty: param.ty.clone(),
                    required: param.required,
                    is_varargs: param.is_varargs,
                    has_runtime_default_expr: param.default_expr.is_some(),
                })
                .collect(),
            param_julia_types: param_julia_types.clone(),
            type_params: type_params.iter().map(Into::into).collect(),
            return_type: return_type.clone(),
            return_julia_type: return_julia_type.clone(),
            vararg_param_index: *vararg_param_index,
            vararg_fixed_count: *vararg_fixed_count,
            min_world: *min_world,
            is_base_extension: *is_base_extension,
            is_generated: *is_generated,
            is_lowering_helper: *is_lowering_helper,
            definition_order: *definition_order,
            inlining_meta: *inlining_meta,
            constprop_meta: *constprop_meta,
            nospecialize_meta: *nospecialize_meta,
            propagate_inbounds_meta: *propagate_inbounds_meta,
            nospecializeinfer_meta: *nospecializeinfer_meta,
            purity_meta: *purity_meta,
            direct_return_type_param: direct_return_type_param.clone(),
            def_line: *def_line,
            suppress_short_name_alias: *suppress_short_name_alias,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IrFunctionSignatureSnapshot {
    pub name: String,
    pub params: Vec<TypedParamSnapshot>,
    pub kwparams: Vec<KwParamSnapshot>,
    pub type_params: Vec<TypeParamSnapshot>,
    pub return_type: Option<JuliaType>,
    pub is_base_extension: bool,
    pub is_runtime_eval: bool,
    /// Enclosing struct of a struct-body `global` helper (Issue #11005): it
    /// changes how the body's `new` compiles, so it is part of the identity.
    pub new_struct_name: Option<String>,
}

impl From<&Function> for IrFunctionSignatureSnapshot {
    fn from(function: &Function) -> Self {
        let Function {
            name,
            params,
            kwparams,
            type_params,
            return_type,
            body: _,
            is_base_extension,
            is_runtime_eval,
            new_struct_name,
            span: _,
        } = function;
        Self {
            name: name.clone(),
            params: params.iter().map(Into::into).collect(),
            kwparams: kwparams.iter().map(Into::into).collect(),
            type_params: type_params.iter().map(Into::into).collect(),
            return_type: return_type.clone(),
            is_base_extension: *is_base_extension,
            is_runtime_eval: *is_runtime_eval,
            new_struct_name: new_struct_name.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SpecializableFunctionSnapshot {
    pub index: usize,
    pub name: String,
    pub fallback_index: usize,
    pub signature: IrFunctionSignatureSnapshot,
    pub semantic_ir_digest: [u8; 32],
}

impl SpecializableFunctionSnapshot {
    fn capture(index: usize, function: &SpecializableFunction) -> Self {
        let SpecializableFunction {
            ir,
            name,
            fallback_index,
        } = function;
        Self {
            index,
            name: name.clone(),
            fallback_index: *fallback_index,
            signature: ir.as_ref().into(),
            semantic_ir_digest: semantic_function_digest(ir),
        }
    }
}

/// Hash the complete semantic Core IR for a runtime-specializable function.
///
/// Serde supplies exhaustive coverage of every current/future Function/Block/
/// Stmt/Expr field. The canonical binary serializer preserves integer widths
/// and float bits, sorts map entries, and replaces typed `Span` structs with a
/// position-independent marker. Bodies and keyword-default expressions remain
/// in the tree. Explicit tags and byte lengths make the encoding unambiguous
/// without depending on Debug output, JSON's numeric domain, or source layout.
fn semantic_function_digest(function: &Function) -> [u8; 32] {
    let bytes = function
        .serialize(SemanticSerializer)
        .expect("Core IR must serialize to canonical semantic bytes");
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher.finalize().into()
}

#[derive(Debug)]
struct SemanticEncodeError(String);

impl std::fmt::Display for SemanticEncodeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for SemanticEncodeError {}

impl serde::ser::Error for SemanticEncodeError {
    fn custom<T: std::fmt::Display>(message: T) -> Self {
        Self(message.to_string())
    }
}

type SemanticResult<T> = Result<T, SemanticEncodeError>;

fn semantic_atom(tag: u8, bytes: &[u8]) -> Vec<u8> {
    let mut encoded = Vec::with_capacity(1 + 8 + bytes.len());
    encoded.push(tag);
    encoded.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
    encoded.extend_from_slice(bytes);
    encoded
}

fn semantic_parts(tag: u8, metadata: &[&str], mut parts: Vec<Vec<u8>>) -> Vec<u8> {
    let mut encoded = vec![tag];
    encoded.extend_from_slice(&(metadata.len() as u64).to_le_bytes());
    for value in metadata {
        let value = semantic_atom(20, value.as_bytes());
        encoded.extend_from_slice(&(value.len() as u64).to_le_bytes());
        encoded.extend_from_slice(&value);
    }
    encoded.extend_from_slice(&(parts.len() as u64).to_le_bytes());
    for part in parts.drain(..) {
        encoded.extend_from_slice(&(part.len() as u64).to_le_bytes());
        encoded.extend_from_slice(&part);
    }
    encoded
}

struct SemanticSerializer;

impl serde::Serializer for SemanticSerializer {
    type Ok = Vec<u8>;
    type Error = SemanticEncodeError;
    type SerializeSeq = SemanticSequence;
    type SerializeTuple = SemanticSequence;
    type SerializeTupleStruct = SemanticSequence;
    type SerializeTupleVariant = SemanticSequence;
    type SerializeMap = SemanticMap;
    type SerializeStruct = SemanticStruct;
    type SerializeStructVariant = SemanticStruct;

    fn is_human_readable(&self) -> bool {
        false
    }

    fn serialize_bool(self, value: bool) -> SemanticResult<Self::Ok> {
        Ok(semantic_atom(1, &[u8::from(value)]))
    }

    fn serialize_i8(self, value: i8) -> SemanticResult<Self::Ok> {
        Ok(semantic_atom(2, &value.to_le_bytes()))
    }

    fn serialize_i16(self, value: i16) -> SemanticResult<Self::Ok> {
        Ok(semantic_atom(3, &value.to_le_bytes()))
    }

    fn serialize_i32(self, value: i32) -> SemanticResult<Self::Ok> {
        Ok(semantic_atom(4, &value.to_le_bytes()))
    }

    fn serialize_i64(self, value: i64) -> SemanticResult<Self::Ok> {
        Ok(semantic_atom(5, &value.to_le_bytes()))
    }

    fn serialize_i128(self, value: i128) -> SemanticResult<Self::Ok> {
        Ok(semantic_atom(6, &value.to_le_bytes()))
    }

    fn serialize_u8(self, value: u8) -> SemanticResult<Self::Ok> {
        Ok(semantic_atom(7, &value.to_le_bytes()))
    }

    fn serialize_u16(self, value: u16) -> SemanticResult<Self::Ok> {
        Ok(semantic_atom(8, &value.to_le_bytes()))
    }

    fn serialize_u32(self, value: u32) -> SemanticResult<Self::Ok> {
        Ok(semantic_atom(9, &value.to_le_bytes()))
    }

    fn serialize_u64(self, value: u64) -> SemanticResult<Self::Ok> {
        Ok(semantic_atom(10, &value.to_le_bytes()))
    }

    fn serialize_u128(self, value: u128) -> SemanticResult<Self::Ok> {
        Ok(semantic_atom(11, &value.to_le_bytes()))
    }

    fn serialize_f32(self, value: f32) -> SemanticResult<Self::Ok> {
        Ok(semantic_atom(12, &value.to_bits().to_le_bytes()))
    }

    fn serialize_f64(self, value: f64) -> SemanticResult<Self::Ok> {
        Ok(semantic_atom(13, &value.to_bits().to_le_bytes()))
    }

    fn serialize_char(self, value: char) -> SemanticResult<Self::Ok> {
        Ok(semantic_atom(14, &u32::from(value).to_le_bytes()))
    }

    fn serialize_str(self, value: &str) -> SemanticResult<Self::Ok> {
        Ok(semantic_atom(15, value.as_bytes()))
    }

    fn serialize_bytes(self, value: &[u8]) -> SemanticResult<Self::Ok> {
        Ok(semantic_atom(16, value))
    }

    fn serialize_none(self) -> SemanticResult<Self::Ok> {
        Ok(vec![17])
    }

    fn serialize_some<T: ?Sized + Serialize>(self, value: &T) -> SemanticResult<Self::Ok> {
        Ok(semantic_parts(18, &[], vec![value.serialize(self)?]))
    }

    fn serialize_unit(self) -> SemanticResult<Self::Ok> {
        Ok(vec![19])
    }

    fn serialize_unit_struct(self, name: &'static str) -> SemanticResult<Self::Ok> {
        Ok(semantic_parts(21, &[name], Vec::new()))
    }

    fn serialize_unit_variant(
        self,
        name: &'static str,
        variant_index: u32,
        variant: &'static str,
    ) -> SemanticResult<Self::Ok> {
        Ok(semantic_parts(
            22,
            &[name, variant],
            vec![semantic_atom(9, &variant_index.to_le_bytes())],
        ))
    }

    fn serialize_newtype_struct<T: ?Sized + Serialize>(
        self,
        name: &'static str,
        value: &T,
    ) -> SemanticResult<Self::Ok> {
        Ok(semantic_parts(23, &[name], vec![value.serialize(self)?]))
    }

    fn serialize_newtype_variant<T: ?Sized + Serialize>(
        self,
        name: &'static str,
        variant_index: u32,
        variant: &'static str,
        value: &T,
    ) -> SemanticResult<Self::Ok> {
        Ok(semantic_parts(
            24,
            &[name, variant],
            vec![
                semantic_atom(9, &variant_index.to_le_bytes()),
                value.serialize(self)?,
            ],
        ))
    }

    fn serialize_seq(self, _len: Option<usize>) -> SemanticResult<Self::SerializeSeq> {
        Ok(SemanticSequence::new(25, Vec::new()))
    }

    fn serialize_tuple(self, _len: usize) -> SemanticResult<Self::SerializeTuple> {
        Ok(SemanticSequence::new(26, Vec::new()))
    }

    fn serialize_tuple_struct(
        self,
        name: &'static str,
        _len: usize,
    ) -> SemanticResult<Self::SerializeTupleStruct> {
        Ok(SemanticSequence::new(27, vec![name]))
    }

    fn serialize_tuple_variant(
        self,
        name: &'static str,
        variant_index: u32,
        variant: &'static str,
        _len: usize,
    ) -> SemanticResult<Self::SerializeTupleVariant> {
        let mut sequence = SemanticSequence::new(28, vec![name, variant]);
        sequence
            .values
            .push(semantic_atom(9, &variant_index.to_le_bytes()));
        Ok(sequence)
    }

    fn serialize_map(self, _len: Option<usize>) -> SemanticResult<Self::SerializeMap> {
        Ok(SemanticMap::default())
    }

    fn serialize_struct(
        self,
        name: &'static str,
        _len: usize,
    ) -> SemanticResult<Self::SerializeStruct> {
        Ok(SemanticStruct::new(30, vec![name], name == "Span"))
    }

    fn serialize_struct_variant(
        self,
        name: &'static str,
        variant_index: u32,
        variant: &'static str,
        _len: usize,
    ) -> SemanticResult<Self::SerializeStructVariant> {
        let mut structure = SemanticStruct::new(31, vec![name, variant], false);
        structure.fields.push((
            "$variant_index".to_string(),
            semantic_atom(9, &variant_index.to_le_bytes()),
        ));
        Ok(structure)
    }
}

struct SemanticSequence {
    tag: u8,
    metadata: Vec<&'static str>,
    values: Vec<Vec<u8>>,
}

impl SemanticSequence {
    fn new(tag: u8, metadata: Vec<&'static str>) -> Self {
        Self {
            tag,
            metadata,
            values: Vec::new(),
        }
    }

    fn push<T: ?Sized + Serialize>(&mut self, value: &T) -> SemanticResult<()> {
        self.values.push(value.serialize(SemanticSerializer)?);
        Ok(())
    }

    fn finish(self) -> Vec<u8> {
        semantic_parts(self.tag, &self.metadata, self.values)
    }
}

impl SerializeSeq for SemanticSequence {
    type Ok = Vec<u8>;
    type Error = SemanticEncodeError;

    fn serialize_element<T: ?Sized + Serialize>(&mut self, value: &T) -> SemanticResult<()> {
        self.push(value)
    }

    fn end(self) -> SemanticResult<Self::Ok> {
        Ok(self.finish())
    }
}

impl SerializeTuple for SemanticSequence {
    type Ok = Vec<u8>;
    type Error = SemanticEncodeError;

    fn serialize_element<T: ?Sized + Serialize>(&mut self, value: &T) -> SemanticResult<()> {
        self.push(value)
    }

    fn end(self) -> SemanticResult<Self::Ok> {
        Ok(self.finish())
    }
}

impl SerializeTupleStruct for SemanticSequence {
    type Ok = Vec<u8>;
    type Error = SemanticEncodeError;

    fn serialize_field<T: ?Sized + Serialize>(&mut self, value: &T) -> SemanticResult<()> {
        self.push(value)
    }

    fn end(self) -> SemanticResult<Self::Ok> {
        Ok(self.finish())
    }
}

impl SerializeTupleVariant for SemanticSequence {
    type Ok = Vec<u8>;
    type Error = SemanticEncodeError;

    fn serialize_field<T: ?Sized + Serialize>(&mut self, value: &T) -> SemanticResult<()> {
        self.push(value)
    }

    fn end(self) -> SemanticResult<Self::Ok> {
        Ok(self.finish())
    }
}

#[derive(Default)]
struct SemanticMap {
    entries: Vec<(Vec<u8>, Vec<u8>)>,
    pending_key: Option<Vec<u8>>,
}

impl SerializeMap for SemanticMap {
    type Ok = Vec<u8>;
    type Error = SemanticEncodeError;

    fn serialize_key<T: ?Sized + Serialize>(&mut self, key: &T) -> SemanticResult<()> {
        if self.pending_key.is_some() {
            return Err(SemanticEncodeError::custom("map key missing a value"));
        }
        self.pending_key = Some(key.serialize(SemanticSerializer)?);
        Ok(())
    }

    fn serialize_value<T: ?Sized + Serialize>(&mut self, value: &T) -> SemanticResult<()> {
        let key = self
            .pending_key
            .take()
            .ok_or_else(|| SemanticEncodeError::custom("map value missing a key"))?;
        self.entries
            .push((key, value.serialize(SemanticSerializer)?));
        Ok(())
    }

    fn end(mut self) -> SemanticResult<Self::Ok> {
        if self.pending_key.is_some() {
            return Err(SemanticEncodeError::custom("map key missing a value"));
        }
        self.entries.sort_unstable();
        let entries = self
            .entries
            .into_iter()
            .map(|(key, value)| semantic_parts(32, &[], vec![key, value]))
            .collect();
        Ok(semantic_parts(29, &[], entries))
    }
}

struct SemanticStruct {
    tag: u8,
    metadata: Vec<&'static str>,
    fields: Vec<(String, Vec<u8>)>,
    normalize_span: bool,
}

impl SemanticStruct {
    fn new(tag: u8, metadata: Vec<&'static str>, normalize_span: bool) -> Self {
        Self {
            tag,
            metadata,
            fields: Vec::new(),
            normalize_span,
        }
    }

    fn push<T: ?Sized + Serialize>(&mut self, key: &'static str, value: &T) -> SemanticResult<()> {
        if !self.normalize_span {
            self.fields
                .push((key.to_string(), value.serialize(SemanticSerializer)?));
        }
        Ok(())
    }

    fn finish(mut self) -> Vec<u8> {
        if self.normalize_span {
            return vec![33];
        }
        self.fields
            .sort_unstable_by(|left, right| left.0.cmp(&right.0));
        let fields = self
            .fields
            .into_iter()
            .map(|(key, value)| semantic_parts(34, &[&key], vec![value]))
            .collect();
        semantic_parts(self.tag, &self.metadata, fields)
    }
}

impl SerializeStruct for SemanticStruct {
    type Ok = Vec<u8>;
    type Error = SemanticEncodeError;

    fn serialize_field<T: ?Sized + Serialize>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> SemanticResult<()> {
        self.push(key, value)
    }

    fn end(self) -> SemanticResult<Self::Ok> {
        Ok(self.finish())
    }
}

impl SerializeStructVariant for SemanticStruct {
    type Ok = Vec<u8>;
    type Error = SemanticEncodeError;

    fn serialize_field<T: ?Sized + Serialize>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> SemanticResult<()> {
        self.push(key, value)
    }

    fn end(self) -> SemanticResult<Self::Ok> {
        Ok(self.finish())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::SpecializableFunctionSnapshot;
    use crate::bytecode::SpecializableFunction;
    use crate::ir::core::{Block, Expr, Literal, Stmt};

    fn user_function(source: &str, name: &str) -> std::sync::Arc<crate::ir::core::Function> {
        let program = crate::pipeline::parse_and_lower(source).expect("test source must lower");
        program
            .functions
            .iter()
            .find(|function| function.name == name)
            .cloned()
            .unwrap_or_else(|| panic!("missing function {name}"))
    }

    fn snapshot(ir: std::sync::Arc<crate::ir::core::Function>) -> SpecializableFunctionSnapshot {
        SpecializableFunctionSnapshot::capture(
            0,
            &SpecializableFunction {
                ir,
                name: "snap10462".to_string(),
                fallback_index: 7,
            },
        )
    }

    fn snapshot_with_return_literal(literal: Literal) -> SpecializableFunctionSnapshot {
        let mut ir = user_function("snap10462() = 0", "snap10462");
        let span = ir.span;
        std::sync::Arc::make_mut(&mut ir).body = Block {
            stmts: vec![Stmt::Return {
                value: Some(Expr::Literal(literal, span)),
                span,
            }],
            span,
        };
        snapshot(ir)
    }

    #[test]
    fn semantic_digest_detects_body_and_kw_default_changes_10462() {
        let baseline = snapshot(user_function("snap10462(x; y=1) = x + y", "snap10462"));
        let body_changed = snapshot(user_function("snap10462(x; y=1) = x - y", "snap10462"));
        let default_changed = snapshot(user_function("snap10462(x; y=2) = x + y", "snap10462"));
        let span_only_changed =
            snapshot(user_function("\n\n snap10462(x; y=1) = x + y", "snap10462"));

        assert_eq!(baseline.signature, body_changed.signature);
        assert_eq!(baseline.signature, default_changed.signature);
        assert_ne!(baseline.semantic_ir_digest, body_changed.semantic_ir_digest);
        assert_ne!(
            baseline.semantic_ir_digest,
            default_changed.semantic_ir_digest
        );
        assert_eq!(baseline, span_only_changed);
    }

    #[test]
    fn semantic_digest_preserves_large_int128_10462() {
        let maximum = snapshot_with_return_literal(Literal::Int128(i128::MAX));
        let adjacent = snapshot_with_return_literal(Literal::Int128(i128::MAX - 1));

        assert_ne!(maximum.semantic_ir_digest, adjacent.semantic_ir_digest);
    }

    #[test]
    fn semantic_digest_preserves_nonfinite_float_bits_10462() {
        let cases = [
            Literal::Float(f64::NAN),
            Literal::Float(f64::INFINITY),
            Literal::Float(f64::NEG_INFINITY),
            Literal::Float(f64::from_bits(f64::NAN.to_bits() + 1)),
            Literal::Float32(f32::NAN),
            Literal::Float32(f32::INFINITY),
            Literal::Float32(f32::NEG_INFINITY),
            Literal::Float32(f32::from_bits(f32::NAN.to_bits() + 1)),
            Literal::Float16(half::f16::NAN),
            Literal::Float16(half::f16::INFINITY),
            Literal::Float16(half::f16::NEG_INFINITY),
            Literal::Float16(half::f16::from_bits(half::f16::NAN.to_bits() + 1)),
            Literal::Array(vec![f64::NAN], vec![1]),
            Literal::Array(vec![f64::INFINITY], vec![1]),
            Literal::Array(vec![f64::NEG_INFINITY], vec![1]),
            Literal::Array(vec![f64::from_bits(f64::NAN.to_bits() + 1)], vec![1]),
        ];
        let digests: std::collections::HashSet<_> = cases
            .into_iter()
            .map(|literal| snapshot_with_return_literal(literal).semantic_ir_digest)
            .collect();

        assert_eq!(digests.len(), 16);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct PromotionRegistrySnapshot {
    pub initialized: bool,
    pub rules: Vec<(String, String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct SpecializationPolicySnapshot {
    pub disable_array_getindex: bool,
    pub disable_array_setindex: bool,
    pub disable_field_access: bool,
    pub runtime_specialization_map: Vec<(usize, usize)>,
    pub specializable_functions: Vec<SpecializableFunctionSnapshot>,
}

impl CompileContextSnapshot {
    pub(crate) fn capture(compiled: &CompiledProgram) -> Self {
        let mut main_scope_names: Vec<_> = compiled.main_scope_names.iter().cloned().collect();
        main_scope_names.sort();
        let mut promotion_rules = crate::promotion::get_all_promotion_rules();
        promotion_rules.sort();
        let mut runtime_specialization_map = compiled.runtime_specialization_map.clone();
        runtime_specialization_map.sort_unstable();

        let mut snapshot = Self {
            runtime_context_present: compiled.compile_context.is_some(),
            semantic_structs: Vec::new(),
            struct_definitions: Vec::new(),
            parametric_structs: Vec::new(),
            type_aliases: Vec::new(),
            inference_global_types: Vec::new(),
            primitive_types: Vec::new(),
            module_registry: Vec::new(),
            main_scope_names,
            method_signatures: compiled
                .functions
                .iter()
                .enumerate()
                .map(|(index, function)| MethodSignatureSnapshot::capture(index, function))
                .collect(),
            promotion_registry: PromotionRegistrySnapshot {
                initialized: crate::promotion::is_registry_initialized(),
                rules: promotion_rules,
            },
            specialization_policy: SpecializationPolicySnapshot {
                runtime_specialization_map,
                specializable_functions: compiled
                    .specializable_functions
                    .iter()
                    .enumerate()
                    .map(|(index, function)| {
                        SpecializableFunctionSnapshot::capture(index, function)
                    })
                    .collect(),
                ..SpecializationPolicySnapshot::default()
            },
        };

        if let Some(context) = &compiled.compile_context {
            let RuntimeCompileContext {
                struct_table,
                struct_defs,
                parametric_structs,
                base_parametric_structs: _,
                type_aliases,
                module_imported_bindings: _,
                module_base_exports_visibility: _,
                module_implicit_standard_bindings: _,
                base_exported_names: _,
                inference_global_types,
                primitive_types,
                disable_array_getindex_specialization,
                disable_array_setindex_specialization,
                disable_field_access_specialization,
                module_registry,
            } = context;

            snapshot.semantic_structs = struct_table
                .iter()
                .map(|(binding, info)| SemanticStructSnapshot {
                    binding: binding.clone(),
                    type_id: info.type_id,
                    definition_name: struct_defs
                        .get(info.type_id)
                        .map(|definition| definition.name.clone()),
                    is_mutable: info.is_mutable,
                    fields: info.fields.clone(),
                    has_inner_constructor: info.has_inner_constructor,
                })
                .collect();
            snapshot.semantic_structs.sort_by(|left, right| {
                (&left.binding, left.type_id).cmp(&(&right.binding, right.type_id))
            });
            snapshot.struct_definitions = struct_defs
                .iter()
                .enumerate()
                .map(|(index, definition)| StructDefinitionSnapshot::capture(index, definition))
                .collect();
            snapshot.parametric_structs = parametric_structs
                .iter()
                .map(|(binding, definition)| {
                    ParametricStructSnapshot::capture(binding, &definition.def)
                })
                .collect();
            snapshot
                .parametric_structs
                .sort_by(|left, right| left.binding.cmp(&right.binding));
            snapshot.type_aliases = type_aliases
                .iter()
                .map(|(name, target)| (name.clone(), target.clone()))
                .collect();
            snapshot.type_aliases.sort();
            snapshot.inference_global_types = inference_global_types
                .iter()
                .map(|(name, ty)| (name.clone(), ty.clone()))
                .collect();
            snapshot
                .inference_global_types
                .sort_by(|left, right| left.0.cmp(&right.0));
            snapshot.primitive_types = primitive_types
                .iter()
                .enumerate()
                .map(|(index, primitive)| PrimitiveTypeSnapshot::capture(index, primitive))
                .collect();
            snapshot.module_registry = module_registry.paths().map(str::to_string).collect();
            snapshot.module_registry.sort();
            snapshot.specialization_policy.disable_array_getindex =
                *disable_array_getindex_specialization;
            snapshot.specialization_policy.disable_array_setindex =
                *disable_array_setindex_specialization;
            snapshot.specialization_policy.disable_field_access =
                *disable_field_access_specialization;
        }

        snapshot
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum CompileContextField {
    RuntimeContextPresence,
    SemanticStructs,
    StructDefinitions,
    ParametricStructs,
    TypeAliases,
    InferenceGlobalTypes,
    PrimitiveTypes,
    ModuleRegistry,
    MainScopeNames,
    MethodSignatures,
    PromotionRegistry,
    SpecializationPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CompileContextFieldValue {
    Bool(bool),
    SemanticStructs(Vec<SemanticStructSnapshot>),
    StructDefinitions(Vec<StructDefinitionSnapshot>),
    ParametricStructs(Vec<ParametricStructSnapshot>),
    TypeAliases(Vec<(String, String)>),
    InferenceGlobalTypes(Vec<(String, ValueType)>),
    PrimitiveTypes(Vec<PrimitiveTypeSnapshot>),
    ModuleRegistry(Vec<String>),
    MainScopeNames(Vec<String>),
    MethodSignatures(Vec<MethodSignatureSnapshot>),
    PromotionRegistry(PromotionRegistrySnapshot),
    SpecializationPolicy(SpecializationPolicySnapshot),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CompileContextMismatch {
    pub field: CompileContextField,
    pub tracking_issue: Option<u64>,
    pub fresh: CompileContextFieldValue,
    pub restored: CompileContextFieldValue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CompileContextScoreboard {
    lane: String,
    mismatches: Vec<CompileContextMismatch>,
}

impl CompileContextScoreboard {
    pub(crate) fn compare(
        lane: impl Into<String>,
        fresh: &CompileContextSnapshot,
        restored: &CompileContextSnapshot,
        tracking_issue: impl Fn(CompileContextField) -> Option<u64>,
    ) -> Self {
        let mut mismatches = Vec::new();
        macro_rules! compare_field {
            ($field:ident, $variant:ident, $left:expr, $right:expr) => {
                if $left != $right {
                    let field = CompileContextField::$field;
                    mismatches.push(CompileContextMismatch {
                        field,
                        tracking_issue: tracking_issue(field),
                        fresh: CompileContextFieldValue::$variant($left.clone()),
                        restored: CompileContextFieldValue::$variant($right.clone()),
                    });
                }
            };
        }

        compare_field!(
            RuntimeContextPresence,
            Bool,
            fresh.runtime_context_present,
            restored.runtime_context_present
        );
        compare_field!(
            SemanticStructs,
            SemanticStructs,
            fresh.semantic_structs,
            restored.semantic_structs
        );
        compare_field!(
            StructDefinitions,
            StructDefinitions,
            fresh.struct_definitions,
            restored.struct_definitions
        );
        compare_field!(
            ParametricStructs,
            ParametricStructs,
            fresh.parametric_structs,
            restored.parametric_structs
        );
        compare_field!(
            TypeAliases,
            TypeAliases,
            fresh.type_aliases,
            restored.type_aliases
        );
        compare_field!(
            InferenceGlobalTypes,
            InferenceGlobalTypes,
            fresh.inference_global_types,
            restored.inference_global_types
        );
        compare_field!(
            PrimitiveTypes,
            PrimitiveTypes,
            fresh.primitive_types,
            restored.primitive_types
        );
        compare_field!(
            ModuleRegistry,
            ModuleRegistry,
            fresh.module_registry,
            restored.module_registry
        );
        compare_field!(
            MainScopeNames,
            MainScopeNames,
            fresh.main_scope_names,
            restored.main_scope_names
        );
        compare_field!(
            MethodSignatures,
            MethodSignatures,
            fresh.method_signatures,
            restored.method_signatures
        );
        compare_field!(
            PromotionRegistry,
            PromotionRegistry,
            fresh.promotion_registry,
            restored.promotion_registry
        );
        compare_field!(
            SpecializationPolicy,
            SpecializationPolicy,
            fresh.specialization_policy,
            restored.specialization_policy
        );

        Self {
            lane: lane.into(),
            mismatches,
        }
    }

    pub(crate) fn mismatches(&self) -> &[CompileContextMismatch] {
        &self.mismatches
    }

    pub(crate) fn render(&self) -> String {
        let mut lines = vec![format!(
            "compile-context parity scoreboard for {}:",
            self.lane
        )];
        for mismatch in &self.mismatches {
            let issue = mismatch.tracking_issue.map_or_else(
                || "UNTRACKED".to_string(),
                |number| format!("Issue #{number}"),
            );
            lines.push(format!(
                "- {:?}: {issue}\n  fresh={:?}\n  restored={:?}",
                mismatch.field, mismatch.fresh, mismatch.restored
            ));
        }
        if self.mismatches.is_empty() {
            lines.push("all fields match".to_string());
        }
        lines.join("\n")
    }
}
