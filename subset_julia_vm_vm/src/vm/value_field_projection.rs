//! Julia physical-field projection used by iteration protocols.
//!
//! Upstream `_apply_iterate` consumes arbitrary `iterate` results through
//! `jl_get_nth_field_checked`, not by requiring a tuple. Keep that projection
//! in one authority so positional splats and keyword `indexed_iterate`
//! destructuring agree on bounds, undefined fields, and modeled composite
//! layouts (Issue #11372).

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use crate::rng::RngLike;
use crate::vm::error::VmError;
use crate::vm::value::{
    ModuleValue, RangeElementType, RangeValue, StructInstance, SymbolValue, Value,
};
use crate::vm::Vm;
use num_traits::{One, Zero};

struct ProjectedField {
    field_count: usize,
    value: Option<Value>,
}

impl ProjectedField {
    fn from_slice(fields: &[Value], index: usize) -> Self {
        Self {
            field_count: fields.len(),
            value: fields.get(index).cloned(),
        }
    }

    fn fixed(field_count: usize, value: Option<Value>) -> Self {
        Self { field_count, value }
    }
}

fn twice_precision_value(type_id: usize, hi: f64, lo: f64) -> Value {
    Value::Struct(StructInstance::with_name(
        type_id,
        "Base.TwicePrecision{Float64}".to_string(),
        vec![Value::F64(hi), Value::F64(lo)],
    ))
}

/// Project the stored first/last field of UnitRange/StepRange.
///
/// The VM retains the source terminal bound, while Julia stores the normalized
/// included endpoint. Empty ordinal ranges use the adjacent sentinel; empty
/// Char ranges preserve the requested terminal codepoint.
fn range_endpoint(range: &RangeValue, first: bool) -> Value {
    if let Some(parts) = &range.bigint {
        if first {
            return Value::BigInt(parts.start.clone());
        }

        let start = parts.start.as_inner().clone();
        let step = parts.step.as_inner().clone();
        let stop = parts.stop.as_inner().clone();
        let zero = num_bigint::BigInt::zero();
        let last = if step > zero {
            if stop >= start {
                let offset = (&stop - &start) / &step;
                start + offset * step
            } else {
                start - num_bigint::BigInt::one()
            }
        } else if step < zero {
            if stop <= start {
                let offset = (&stop - &start) / &step;
                start + offset * step
            } else {
                start + num_bigint::BigInt::one()
            }
        } else {
            stop
        };
        return Value::BigInt(crate::vm::value::RustBigInt::from(last));
    }

    if first {
        return range.typed_element(range.start);
    }

    let length = range.length();
    let last = if length > 0 {
        range.start + (length - 1) as f64 * range.step
    } else if matches!(range.element_type, RangeElementType::Char) {
        range.stop
    } else if range.step > 0.0 {
        range.start - 1.0
    } else if range.step < 0.0 {
        range.start + 1.0
    } else {
        range.stop
    };
    range.typed_element(last)
}

fn range_projected_field(
    range: &RangeValue,
    index: usize,
    twice_precision_type_id: usize,
) -> Result<ProjectedField, VmError> {
    if range.is_explicit_float_type() {
        let hp = range.float_hp().ok_or_else(|| {
            VmError::InternalError(
                "StepRangeLen physical fields require TwicePrecision parts".to_string(),
            )
        })?;
        let narrow_accumulator = matches!(
            range.element_type,
            RangeElementType::Float16 | RangeElementType::Float32
        );
        let ref_value = if narrow_accumulator {
            Value::F64(hp.ref_.to_f64())
        } else {
            twice_precision_value(twice_precision_type_id, hp.ref_.hi, hp.ref_.lo)
        };
        let step_value = if narrow_accumulator {
            Value::F64(hp.step.to_f64())
        } else {
            twice_precision_value(twice_precision_type_id, hp.step.hi, hp.step.lo)
        };
        let value = match index {
            0 => Some(ref_value),
            1 => Some(step_value),
            2 => Some(Value::I64(range.length())),
            3 => Some(Value::I64(hp.offset)),
            _ => None,
        };
        return Ok(ProjectedField::fixed(4, value));
    }

    let is_step_range =
        matches!(range.element_type, RangeElementType::Char) || !range.is_unit_range();
    let field_count = if is_step_range { 3 } else { 2 };
    let value = if is_step_range {
        match index {
            0 => Some(range_endpoint(range, true)),
            1 => Some(range.typed_step()),
            2 => Some(range_endpoint(range, false)),
            _ => None,
        }
    } else {
        match index {
            0 => Some(range_endpoint(range, true)),
            1 => Some(range_endpoint(range, false)),
            _ => None,
        }
    };
    Ok(ProjectedField::fixed(field_count, value))
}

impl<R: RngLike> Vm<R> {
    fn projected_struct_type_id(&self, concrete_name: &str) -> usize {
        let unqualified = concrete_name.strip_prefix("Base.");
        self.struct_defs
            .iter()
            .position(|def| {
                def.name == concrete_name || unqualified.is_some_and(|name| def.name == name)
            })
            // A synthetic native-layout value must never alias an unrelated
            // struct definition when Base did not register its concrete type.
            .unwrap_or(usize::MAX)
    }

    fn project_julia_field(&self, value: &Value, index: usize) -> Result<ProjectedField, VmError> {
        let projected = match value {
            Value::Tuple(tuple) => ProjectedField::from_slice(&tuple.elements, index),
            Value::NamedTuple(named) => ProjectedField::from_slice(&named.values, index),
            // Core.SimpleVector is an opaque runtime carrier with zero Julia
            // object fields. Its positional-splat fast path is separate.
            Value::SimpleVector(_) => ProjectedField::fixed(0, None),
            Value::Struct(instance) => ProjectedField::from_slice(&instance.values, index),
            Value::StructRef(heap_index) => {
                let instance = self.struct_heap.get(*heap_index).ok_or_else(|| {
                    VmError::InternalError(format!(
                        "invalid StructRef during physical field projection: index {} out of bounds",
                        heap_index
                    ))
                })?;
                ProjectedField::from_slice(&instance.values, index)
            }
            Value::Range(range) => {
                let twice_precision_type_id =
                    self.projected_struct_type_id("Base.TwicePrecision{Float64}");
                return range_projected_field(range, index, twice_precision_type_id);
            }
            Value::StaticArray(array) => ProjectedField::fixed(
                1,
                (index == 0).then(|| Value::Tuple(array.to_tuple_value())),
            ),
            Value::StaticArrayInline(array) => ProjectedField::fixed(
                1,
                (index == 0).then(|| Value::Tuple(array.to_tuple_value())),
            ),
            // Every current PairsValue origin is Base.Pairs backed by a
            // NamedTuple, whose physical layout is `(data, nothing)` (#11380).
            Value::Pairs(pairs) => ProjectedField::fixed(
                2,
                match index {
                    0 => Some(Value::NamedTuple(pairs.data.clone())),
                    1 => Some(Value::Nothing),
                    _ => None,
                },
            ),
            Value::Ref(cell) | Value::WeakRef(cell) => {
                ProjectedField::fixed(1, (index == 0).then(|| cell.borrow().clone()))
            }
            Value::Closure(closure) => ProjectedField::fixed(
                closure.captures.len(),
                closure.captures.get(index).map(|(_, value)| value.clone()),
            ),
            Value::ComposedFunction(composed) => ProjectedField::fixed(
                2,
                match index {
                    0 => Some((*composed.outer).clone()),
                    1 => Some((*composed.inner).clone()),
                    _ => None,
                },
            ),
            Value::RuntimeTypeVar(typevar) => ProjectedField::fixed(
                3,
                match index {
                    0 => Some(Value::Symbol(SymbolValue::new(&typevar.name))),
                    1 => Some(Value::type_object(typevar.lower_bound.clone())),
                    2 => Some(Value::type_object(typevar.upper_bound.clone())),
                    _ => None,
                },
            ),
            Value::Expr(expr) => ProjectedField::fixed(
                2,
                match index {
                    0 => Some(Value::Symbol(expr.head.clone())),
                    1 => Some(expr.get_args()),
                    _ => None,
                },
            ),
            Value::QuoteNode(inner) => {
                ProjectedField::fixed(1, (index == 0).then(|| (**inner).clone()))
            }
            Value::LineNumberNode(line) => ProjectedField::fixed(
                2,
                match index {
                    0 => Some(Value::I64(line.line)),
                    1 => Some(
                        line.file
                            .as_ref()
                            .map_or(Value::Nothing, |file| Value::Symbol(SymbolValue::new(file))),
                    ),
                    _ => None,
                },
            ),
            Value::GlobalRef(global_ref) => ProjectedField::fixed(
                3,
                match index {
                    0 => Some(Value::Module(Box::new(ModuleValue::new(
                        &global_ref.module,
                    )))),
                    1 => Some(Value::Symbol(global_ref.name.clone())),
                    2 => Some(Value::Binding(Box::new(
                        crate::vm::value::BindingValue::new(global_ref.clone()),
                    ))),
                    _ => None,
                },
            ),
            // `Base.Generator{I,F}`: `f::F` then `iter::I` (Issue #11382),
            // projected through the same authority `getfield`/`fieldnames`
            // use (`generator_projected_field_by_index`) so this path cannot
            // silently drift from those.
            Value::Generator(generator) => ProjectedField::fixed(
                2,
                match index {
                    0 | 1 => self.generator_projected_field_by_index(generator, index)?,
                    _ => None,
                },
            ),
            // `RegexMatch`'s five upstream physical fields (Issue #11382),
            // projected through the shared `RegexMatchValue::field_by_index`
            // authority so this path, `getfield`, and dot-access agree.
            Value::RegexMatch(m) => ProjectedField::fixed(5, m.field_by_index(index)?),
            // These are genuine zero-field Julia values, not merely carriers
            // whose host representation happens not to expose its fields.
            Value::I8(_)
            | Value::I16(_)
            | Value::I32(_)
            | Value::I64(_)
            | Value::I128(_)
            | Value::U8(_)
            | Value::U16(_)
            | Value::U32(_)
            | Value::U64(_)
            | Value::U128(_)
            | Value::F16(_)
            | Value::F32(_)
            | Value::F64(_)
            | Value::Bool(_)
            | Value::Str(_)
            | Value::StrBytes(_)
            | Value::Char(_)
            | Value::CharMalformed(_)
            | Value::Nothing
            | Value::Missing
            | Value::SliceAll
            | Value::Function(_)
            | Value::Module(_)
            | Value::Symbol(_)
            | Value::Enum { .. } => ProjectedField::fixed(0, None),
            Value::Undef => return Err(VmError::UndefRefError),
            // GenericMemory/MemoryRef and the VM's Expr-array carrier contain
            // pointer/storage fields the Value model cannot reconstruct.
            Value::Memory(_) | Value::MemoryRef(_) | Value::ExprArgs(_) => {
                return Err(VmError::NotImplemented(
                    "physical field projection for Memory/Array carriers (Issue #11377)"
                        .to_string(),
                ));
            }
            // These Julia objects have nonzero physical layouts, but their
            // Rust-backed carriers omit, collapse, or reinterpret fields. Fail
            // closed rather than reporting a false zero-field BoundsError.
            // `Regex` itself is a genuine remaining gap here (Issue #11382):
            // upstream's `compile_options`/`match_options` bit flags and the
            // opaque compiled-pattern pointer have no sjulia-side value to
            // project, unlike `RegexMatch` (above), which stores everything
            // it needs.
            Value::BigInt(_)
            | Value::BigFloat(_)
            | Value::Rng(_)
            | Value::DataType(_)
            | Value::RuntimeTypeName(_)
            | Value::IO(_)
            | Value::Binding(_)
            | Value::Regex(_) => {
                return Err(VmError::NotImplemented(
                    "physical field projection for this Rust-backed composite (Issue #11382)"
                        .to_string(),
                ));
            }
        };
        Ok(projected)
    }

    /// Return a clone of the zero-based Julia physical field, matching
    /// `jl_get_nth_field_checked` bounds/undef behavior (Issue #11372).
    pub(in crate::vm) fn julia_nth_field_checked(
        &self,
        value: &Value,
        index: usize,
    ) -> Result<Value, VmError> {
        let projected = self.project_julia_field(value, index)?;
        let field = projected.value.ok_or(VmError::TupleIndexOutOfBounds {
            index: i64::try_from(index).unwrap_or(i64::MAX).saturating_add(1),
            length: projected.field_count,
        })?;
        if matches!(field, Value::Undef) {
            return Err(VmError::UndefRefError);
        }
        Ok(field)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use crate::rng::StableRng;
    use crate::vm::value::{
        new_memory_ref, ArrayElementType, MemoryValue, NamedTupleValue, PairsValue,
    };

    #[test]
    fn projects_normalized_range_physical_layouts_11372() {
        let vm = Vm::new(Vec::new(), StableRng::new(0));

        let unit = Value::Range(RangeValue::unit_range(1.0, 3.0));
        assert!(matches!(
            vm.julia_nth_field_checked(&unit, 1),
            Ok(Value::I64(3))
        ));
        let empty_unit = Value::Range(RangeValue::unit_range(5.0, 1.0));
        assert!(matches!(
            vm.julia_nth_field_checked(&empty_unit, 1),
            Ok(Value::I64(4))
        ));

        let unaligned_step = Value::Range(RangeValue::step_range(1.0, 2.0, 6.0));
        assert!(matches!(
            vm.julia_nth_field_checked(&unaligned_step, 2),
            Ok(Value::I64(5))
        ));
        let empty_step = Value::Range(RangeValue::step_range(5.0, 2.0, 1.0));
        assert!(matches!(
            vm.julia_nth_field_checked(&empty_step, 2),
            Ok(Value::I64(4))
        ));
        let empty_descending = Value::Range(RangeValue::step_range(1.0, -2.0, 5.0));
        assert!(matches!(
            vm.julia_nth_field_checked(&empty_descending, 2),
            Ok(Value::I64(2))
        ));

        let big_step = Value::Range(RangeValue::bigint_range(
            crate::vm::value::RustBigInt::from(1_i64),
            crate::vm::value::RustBigInt::from(2_i64),
            crate::vm::value::RustBigInt::from(6_i64),
            true,
            RangeElementType::BigInt,
            RangeElementType::BigInt,
        ));
        assert!(matches!(
            vm.julia_nth_field_checked(&big_step, 2),
            Ok(Value::BigInt(value)) if value == crate::vm::value::RustBigInt::from(5_i64)
        ));

        let mut empty_char = RangeValue::step_range('e' as u32 as f64, 2.0, 'a' as u32 as f64);
        empty_char.element_type = RangeElementType::Char;
        empty_char.step_type = RangeElementType::Int64;
        assert!(matches!(
            vm.julia_nth_field_checked(&Value::Range(empty_char), 2),
            Ok(Value::Char('a'))
        ));

        let lin = Value::Range(RangeValue::float_linspace(
            1.0,
            2.0,
            3,
            RangeElementType::Float64,
        ));
        match vm.julia_nth_field_checked(&lin, 0) {
            Ok(Value::Struct(instance)) => assert_eq!(instance.type_id, usize::MAX),
            other => panic!("expected synthetic TwicePrecision, got {other:?}"),
        }
        assert!(matches!(
            vm.julia_nth_field_checked(&lin, 2),
            Ok(Value::I64(3))
        ));
    }

    #[test]
    fn pairs_projects_data_and_nothing_11380() {
        let vm = Vm::new(Vec::new(), StableRng::new(0));
        let pairs = Value::Pairs(PairsValue::from_named_tuple(NamedTupleValue {
            names: vec!["a".to_string()],
            values: vec![Value::I64(1)],
        }));
        assert!(matches!(
            vm.julia_nth_field_checked(&pairs, 0),
            Ok(Value::NamedTuple(_))
        ));
        assert!(matches!(
            vm.julia_nth_field_checked(&pairs, 1),
            Ok(Value::Nothing)
        ));
    }

    #[test]
    fn projects_representable_runtime_composites_11372() {
        use crate::types::JuliaType;
        use crate::vm::value::{
            ClosureValue, ComposedFunctionValue, FunctionValue, RuntimeTypeVarValue,
        };

        let vm = Vm::new(Vec::new(), StableRng::new(0));
        let closure = Value::Closure(ClosureValue::new(
            "capture",
            vec![("x".to_string(), Value::I64(7))],
        ));
        assert!(matches!(
            vm.julia_nth_field_checked(&closure, 0),
            Ok(Value::I64(7))
        ));

        let composed = Value::ComposedFunction(ComposedFunctionValue::new(
            Value::Function(FunctionValue::new("outer")),
            Value::Function(FunctionValue::new("inner")),
        ));
        assert!(matches!(
            vm.julia_nth_field_checked(&composed, 1),
            Ok(Value::Function(function)) if function.name == "inner"
        ));

        let typevar = Value::RuntimeTypeVar(Box::new(RuntimeTypeVarValue {
            id: 1,
            name: "T".to_string(),
            lower_bound: JuliaType::Int64,
            upper_bound: JuliaType::Real,
        }));
        assert!(matches!(
            vm.julia_nth_field_checked(&typevar, 0),
            Ok(Value::Symbol(name)) if name.as_str() == "T"
        ));
        assert!(matches!(
            vm.julia_nth_field_checked(&typevar, 2),
            Ok(Value::DataType(upper)) if *upper == JuliaType::Real
        ));
    }

    #[test]
    fn simple_vector_has_zero_physical_fields_11372() {
        let vm = Vm::new(Vec::new(), StableRng::new(0));
        let value = Value::SimpleVector(crate::vm::value::TupleValue::new(vec![
            Value::I64(1),
            Value::I64(2),
        ]));
        assert!(matches!(
            vm.julia_nth_field_checked(&value, 0),
            Err(VmError::TupleIndexOutOfBounds {
                index: 1,
                length: 0
            })
        ));
    }

    #[test]
    fn invalid_struct_ref_is_internal_corruption_11372() {
        let vm = Vm::new(Vec::new(), StableRng::new(0));
        assert!(matches!(
            vm.julia_nth_field_checked(&Value::StructRef(99), 0),
            Err(VmError::InternalError(_))
        ));
    }

    #[test]
    fn unmodeled_composites_fail_closed_11382() {
        let vm = Vm::new(Vec::new(), StableRng::new(0));
        assert!(matches!(
            vm.julia_nth_field_checked(&Value::BigInt(crate::vm::value::RustBigInt::from(1_i64)), 0),
            Err(VmError::NotImplemented(message)) if message.contains("11382")
        ));
        // `Regex` itself remains fail-closed (Issue #11382): unlike
        // `RegexMatch`, it has no sjulia-side value for the upstream
        // `compile_options`/`match_options` bit flags or opaque pattern
        // pointer.
        let regex = crate::vm::value::RegexValue::new("x", "").expect("valid pattern");
        assert!(matches!(
            vm.julia_nth_field_checked(&Value::Regex(Box::new(regex)), 0),
            Err(VmError::NotImplemented(message)) if message.contains("11382")
        ));
    }

    #[test]
    fn generator_and_regexmatch_project_real_fields_11382() {
        use crate::vm::value::{
            FunctionValue, GeneratorCallable, GeneratorValue, RangeValue, RegexMatchValue,
            RegexValue,
        };

        let vm = Vm::new(Vec::new(), StableRng::new(0));

        // `Base.Generator{I,F}`: field 0 is `f`, field 1 is `iter` (Issue #11382).
        let generator = Value::Generator(Box::new(GeneratorValue::with_result_element_type(
            GeneratorCallable::RuntimeValue(Box::new(Value::Function(FunctionValue::new("f")))),
            Value::Range(RangeValue::unit_range(1.0, 3.0)),
            None,
        )));
        assert!(matches!(
            vm.julia_nth_field_checked(&generator, 0),
            Ok(Value::Function(f)) if f.name == "f"
        ));
        assert!(matches!(
            vm.julia_nth_field_checked(&generator, 1),
            Ok(Value::Range(_))
        ));
        assert!(matches!(
            vm.julia_nth_field_checked(&generator, 2),
            Err(VmError::TupleIndexOutOfBounds {
                index: 3,
                length: 2
            })
        ));

        // `RegexMatch`'s five upstream physical fields, in declaration order.
        let regex = RegexValue::new("x", "").expect("valid pattern");
        let m = Value::RegexMatch(Box::new(RegexMatchValue {
            match_str: "x".to_string(),
            captures: vec![],
            offset: 1,
            offsets: vec![],
            capture_names: vec![],
            regex,
        }));
        assert!(matches!(
            vm.julia_nth_field_checked(&m, 0),
            Ok(Value::Str(s)) if &*s == "x"
        ));
        assert!(matches!(
            vm.julia_nth_field_checked(&m, 2),
            Ok(Value::I64(1))
        ));
        assert!(matches!(
            vm.julia_nth_field_checked(&m, 4),
            Ok(Value::Regex(_))
        ));
        assert!(matches!(
            vm.julia_nth_field_checked(&m, 5),
            Err(VmError::TupleIndexOutOfBounds {
                index: 6,
                length: 5
            })
        ));
    }

    #[test]
    fn memory_physical_projection_is_explicitly_deferred_11377() {
        let vm = Vm::new(Vec::new(), StableRng::new(0));
        let memory = Value::Memory(new_memory_ref(MemoryValue::undef_typed(
            &ArrayElementType::Any,
            1,
        )));
        assert!(matches!(
            vm.julia_nth_field_checked(&memory, 0),
            Err(VmError::NotImplemented(message)) if message.contains("11377")
        ));
    }
}
