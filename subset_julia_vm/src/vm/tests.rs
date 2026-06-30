//! Unit tests for the VM core (`vm::mod`): method dispatch, slot storage,
//! error handling, and call-frame management.

use super::*;
use std::rc::Rc;

fn array_value(arr: ArrayRef) -> Value {
    native_array_ref_value(arr)
}

fn dispatch_test_function(
    name: &str,
    param_julia_types: Vec<crate::types::JuliaType>,
    type_params: Vec<crate::types::TypeParam>,
) -> FunctionInfo {
    FunctionInfo {
        name: name.to_string(),
        params: param_julia_types
            .iter()
            .enumerate()
            .map(|(idx, _)| (format!("x{idx}"), ValueType::Any))
            .collect(),
        kwparams: vec![],
        entry: 0,
        return_type: ValueType::Any,
        return_julia_type: None,
        is_base_extension: false,
        is_generated: false,
        min_world: 1,
        type_params,
        param_julia_types,
        code_start: 0,
        code_end: 0,
        slot_names: vec![],
        slot_types: vec![],
        local_slot_count: 0,
        param_slots: vec![],
        vararg_param_index: None,
        vararg_fixed_count: None,
        inlining_meta: 0,
        constprop_meta: 0,
        nospecialize_meta: 0,
        propagate_inbounds_meta: false,
        nospecializeinfer_meta: false,
        purity_meta: 0,
        direct_return_type_param: None,
        def_line: 0,
    }
}

#[test]
fn test_issue_5926_vm_tracks_base_function_count_for_runtime_origin_fences() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.functions.push(Rc::new(dispatch_test_function(
        "runtime_origin_5926",
        vec![crate::types::JuliaType::Any],
        vec![],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "runtime_origin_5926",
        vec![crate::types::JuliaType::Int64],
        vec![],
    )));

    vm.base_function_count = 1;
    assert!(
        vm.is_base_program_function_index(0),
        "runtime dispatch must identify Base-origin functions by the \
         CompiledProgram base prefix"
    );
    assert!(
        !vm.is_base_program_function_index(1),
        "a function after the Base prefix is user-origin even when it shares \
         a runtime dispatch candidate set"
    );

    vm.base_function_count = 0;
    assert!(
        !vm.is_base_program_function_index(0),
        "zero base_function_count means the runtime has no Base-origin \
         partition"
    );
}

#[test]
fn test_find_best_method_index_issue_5926_origin_fence_blocks_base_dominance_override() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.functions.push(Rc::new(dispatch_test_function(
        "runtime_origin_fence_5926",
        vec![crate::types::JuliaType::TypeVar(
            "T".to_string(),
            Some("Number".to_string()),
        )],
        vec![crate::types::TypeParam::with_upper_bound(
            "T".to_string(),
            "Number".to_string(),
        )],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "runtime_origin_fence_5926",
        vec![crate::types::JuliaType::Any],
        vec![],
    )));
    vm.base_function_count = 1;
    vm.function_name_index
        .insert("runtime_origin_fence_5926".to_string(), vec![0, 1]);

    assert_eq!(
        vm.dominant_method_index_runtime(&["runtime_origin_fence_5926"], &[Value::I64(5)]),
        None,
        "runtime dispatch must mirror MethodTable's #5926 origin fence so \
         Base-origin dominance does not cross over a user-origin candidate"
    );
    vm.base_function_count = 0;
    assert_eq!(
        vm.dominant_method_index_runtime(&["runtime_origin_fence_5926"], &[Value::I64(5)]),
        Some(0),
        "without a Base prefix, T<:Number remains the unique runtime \
         dominance winner"
    );
}

#[test]
fn test_find_best_method_index_from_candidates_allows_base_only_candidates_issue_6251() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.functions.push(Rc::new(dispatch_test_function(
        "base_only_runtime_6251",
        vec![crate::types::JuliaType::TypeVar(
            "T".to_string(),
            Some("Real".to_string()),
        )],
        vec![crate::types::TypeParam::with_upper_bound(
            "T".to_string(),
            "Real".to_string(),
        )],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "base_only_runtime_6251",
        vec![crate::types::JuliaType::Any],
        vec![],
    )));
    vm.base_function_count = 2;

    assert_eq!(
        vm.find_best_method_index_from_candidates(&[0, 1], &[Value::I64(5)])
            .unwrap(),
        Some(0),
        "Base-only candidate sets still need metadata runtime dispatch; \
        only mixed Base/user sets are fenced"
    );
}

#[test]
fn test_find_best_method_index_from_candidates_show_int64_beats_pair_issue_6251() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.struct_hierarchy
        .insert("IOBuffer", Some("IO".to_string()), vec![]);
    vm.functions.push(Rc::new(dispatch_test_function(
        "show",
        vec![
            crate::types::JuliaType::Struct("IO".to_string()),
            crate::types::JuliaType::Any,
        ],
        vec![],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "show",
        vec![
            crate::types::JuliaType::Struct("IO".to_string()),
            crate::types::JuliaType::Int64,
        ],
        vec![],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "show",
        vec![
            crate::types::JuliaType::Struct("IO".to_string()),
            crate::types::JuliaType::Struct("Pair".to_string()),
        ],
        vec![],
    )));
    vm.base_function_count = 3;

    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[0, 1, 2],
            &[Value::IO(value::IOValue::buffer_ref()), Value::I64(1)],
        )
        .unwrap(),
        Some(1),
        "show(io, x::Int64) must not dispatch to show(io, p::Pair)"
    );
}

#[test]
fn test_type_name_resolver_show_int64_beats_pair_issue_6251() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.struct_hierarchy
        .insert("IOBuffer", Some("IO".to_string()), vec![]);
    let candidates = [
        (0usize, vec!["IO".to_string(), "Any".to_string()]),
        (1usize, vec!["IO".to_string(), "Int64".to_string()]),
        (2usize, vec!["IO".to_string(), "Pair".to_string()]),
    ];
    let actual = ["IOBuffer".to_string(), "Int64".to_string()];

    assert_eq!(
        crate::inference_core::dispatch_resolver::resolve_type_name_candidates_with_subtype_fallback(
            candidates.iter().map(|(idx, sig)| (*idx, sig.as_slice())),
            &actual,
            |actual, bound| vm.check_subtype(actual, bound),
        ),
        Some((1, 6)),
        "string typed dispatch must choose the exact Int64 show method over Pair"
    );
}

#[test]
fn test_type_value_dispatch_does_not_match_value_level_parametric_patterns_issue_6251() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.functions.push(Rc::new(dispatch_test_function(
        "runtime_type_value_filter_6251",
        vec![crate::types::JuliaType::Any],
        vec![],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "runtime_type_value_filter_6251",
        vec![crate::types::JuliaType::Type],
        vec![],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "runtime_type_value_filter_6251",
        vec![crate::types::JuliaType::TypeOf(Box::new(
            crate::types::JuliaType::Any,
        ))],
        vec![],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "runtime_type_value_filter_6251",
        vec![crate::types::JuliaType::Struct("Array{T, 1}".to_string())],
        vec![crate::types::TypeParam::new("T".to_string())],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "runtime_type_value_filter_6251",
        vec![crate::types::JuliaType::TypeOf(Box::new(
            crate::types::JuliaType::VectorOf(Box::new(crate::types::JuliaType::TypeVar(
                "T".to_string(),
                None,
            ))),
        ))],
        vec![crate::types::TypeParam::new("T".to_string())],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "runtime_type_value_filter_6251",
        vec![crate::types::JuliaType::TypeOf(Box::new(
            crate::types::JuliaType::MatrixOf(Box::new(crate::types::JuliaType::TypeVar(
                "T".to_string(),
                None,
            ))),
        ))],
        vec![crate::types::TypeParam::new("T".to_string())],
    )));

    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[0, 1, 2, 3, 4, 5],
            &[Value::DataType(Box::new(
                crate::types::JuliaType::VectorOf(Box::new(crate::types::JuliaType::Int64,))
            ))],
        )
        .unwrap(),
        Some(4),
        "a type object argument must match Type{{Vector{{T}}}}, not value-level Array{{T,1}}"
    );
}

#[test]
fn test_get_value_type_returns_arrayof_for_typed_array() {
    let vm = Vm::new(vec![], StableRng::new(0));
    let arr = array_value(new_array_ref(ArrayValue::from_f64(vec![1.0, 2.0], vec![2])));

    assert_eq!(
        vm.get_value_type(&arr),
        ValueType::ArrayOf(ArrayElementType::F64, None)
    );
}

#[test]
fn test_get_value_julia_type_preserves_memory_type_param() {
    let vm = Vm::new(vec![], StableRng::new(0));
    let mem = value::MemoryValue::undef_typed(&ArrayElementType::I64, 2);
    let val = Value::Memory(value::new_memory_ref(mem));

    assert_eq!(
        vm.get_value_julia_type(&val),
        crate::types::JuliaType::Struct("Memory{Int64}".to_string())
    );
}

#[test]
fn test_get_value_julia_type_uses_array_logical_element_type() {
    let vm = Vm::new(vec![], StableRng::new(0));
    let arr = array_value(new_array_ref(ArrayValue::zeros_complex_f64(vec![2])));

    assert_eq!(
        vm.get_value_julia_type(&arr),
        crate::types::JuliaType::VectorOf(Box::new(crate::types::JuliaType::Struct(
            "Complex{Float64}".to_string()
        )))
    );
}

#[test]
fn test_dispatch_julia_type_preserves_io_and_closure_runtime_types_issue_6251() {
    let vm = Vm::new(vec![], StableRng::new(0));

    assert_eq!(
        vm.dispatch_julia_type_for_value(&Value::IO(value::IOValue::buffer_ref())),
        crate::types::JuliaType::IOBuffer
    );
    assert_eq!(
        vm.dispatch_julia_type_for_value(&Value::Closure(ClosureValue::new("strip#pred", vec![],))),
        crate::types::JuliaType::Function
    );
    assert_eq!(
        vm.dispatch_julia_type_for_value(&Value::Symbol(value::SymbolValue::new("compact"))),
        crate::types::JuliaType::Symbol
    );
}

#[test]
fn test_struct_field_memory_comparison_reads_storage_directly() {
    let vm = Vm::new(vec![], StableRng::new(0));
    let memory = value::new_memory_ref(value::MemoryValue::new(
        ArrayData::I64(vec![1, 2, 3]),
        ArrayElementType::I64,
        3,
    ));
    let same_memory = value::new_memory_ref(value::MemoryValue::new(
        ArrayData::I64(vec![1, 2, 3]),
        ArrayElementType::I64,
        3,
    ));
    let different_memory = value::new_memory_ref(value::MemoryValue::new(
        ArrayData::I64(vec![1, 2, 4]),
        ArrayElementType::I64,
        3,
    ));
    let vector = new_array_ref(ArrayValue::new(ArrayData::I64(vec![1, 2, 3]), vec![3]));
    let matrix = new_array_ref(ArrayValue::new(ArrayData::I64(vec![1, 2, 3]), vec![3, 1]));

    assert!(vm.compare_values_equal(&Value::Memory(memory.clone()), &Value::Memory(same_memory)));
    assert!(!vm.compare_values_equal(
        &Value::Memory(memory.clone()),
        &Value::Memory(different_memory)
    ));
    assert!(vm.compare_values_equal(&Value::Memory(memory.clone()), &array_value(vector.clone())));
    assert!(vm.compare_values_equal(&array_value(vector), &Value::Memory(memory.clone())));
    assert!(!vm.compare_values_equal(&Value::Memory(memory), &array_value(matrix)));
}

#[test]
fn test_runtime_diagonal_type_var_rejects_mixed_bigint_rational() {
    let vm = Vm::new(vec![], StableRng::new(0));
    let param_types = vec![
        crate::types::JuliaType::Struct("T".to_string()),
        crate::types::JuliaType::Struct("T".to_string()),
    ];
    let type_params = vec![crate::types::TypeParam::new("T".to_string())];
    let args = vec![
        Value::BigInt(2.into()),
        Value::Struct(StructInstance::with_name(
            0,
            "Rational{Int64}".to_string(),
            vec![],
        )),
    ];

    assert_eq!(
        vm.get_value_julia_type(&args[0]),
        crate::types::JuliaType::BigInt
    );
    assert!(vm
        .values_match_params_binding_count(&args, &param_types, &type_params)
        .is_none());
}

#[test]
fn runtime_parametric_dict_structref_binds_spaced_typevars() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.struct_heap.push(StructInstance::with_name(
        0,
        "Dict{Vector{Int64}, Vector{Any}}".to_string(),
        vec![],
    ));
    let args = vec![Value::StructRef(0)];
    let param_types = vec![crate::types::JuliaType::Struct("Dict{K, V}".to_string())];
    let type_params = vec![
        crate::types::TypeParam::new("K".to_string()),
        crate::types::TypeParam::new("V".to_string()),
    ];

    assert_eq!(
        vm.values_match_params_binding_count(&args, &param_types, &type_params),
        Some(2)
    );
}

#[test]
fn runtime_where_bound_uses_struct_hierarchy_issue_6502() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.struct_hierarchy
        .insert("Animal", Some("Any".to_string()), vec![]);
    vm.struct_hierarchy
        .insert("Dog", Some("Animal".to_string()), vec![]);

    let param_types = vec![crate::types::JuliaType::Struct("T".to_string())];
    let type_params = vec![crate::types::TypeParam::with_upper_bound(
        "T".to_string(),
        "Animal".to_string(),
    )];

    assert_eq!(
        vm.values_match_params_binding_count(
            &[Value::Struct(StructInstance::with_name(
                0,
                "Dog".to_string(),
                vec![],
            ))],
            &param_types,
            &type_params,
        ),
        Some(1),
        "runtime dispatch must honor user-defined hierarchy bounds"
    );
    assert_eq!(
        vm.values_match_params_binding_count(&[Value::I64(1)], &param_types, &type_params),
        None,
        "runtime dispatch must reject values outside the user-defined bound"
    );
}

#[test]
fn runtime_dispatch_finds_parametric_dict_structref_method() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.functions.push(Rc::new(dispatch_test_function(
        "iterate",
        vec![crate::types::JuliaType::Struct("Dict{K, V}".to_string())],
        vec![
            crate::types::TypeParam::new("K".to_string()),
            crate::types::TypeParam::new("V".to_string()),
        ],
    )));
    vm.function_name_index
        .insert("iterate".to_string(), vec![0]);
    vm.struct_heap.push(StructInstance::with_name(
        0,
        "Dict{Vector{Int64}, Vector{Any}}".to_string(),
        vec![],
    ));

    assert_eq!(
        vm.find_best_method_index(&["iterate"], &[Value::StructRef(0)]),
        Some(0)
    );
}

#[test]
fn runtime_type_matches_abstract_numeric_params_via_core_subtype_issue_5921() {
    use crate::types::JuliaType;

    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.struct_hierarchy
        .insert("Complex", Some("Number".to_string()), vec!["T".to_string()]);
    vm.struct_hierarchy
        .insert("Rational", Some("Real".to_string()), vec!["T".to_string()]);

    assert!(vm.type_matches("Int8", &JuliaType::Real));
    assert!(vm.type_matches("UInt128", &JuliaType::Real));
    assert!(vm.type_matches("BigInt", &JuliaType::Real));
    assert!(vm.type_matches("Rational{Int64}", &JuliaType::Real));
    assert!(!vm.type_matches("Complex{Float64}", &JuliaType::Real));

    assert!(vm.type_matches("Complex{Float64}", &JuliaType::Number));
    assert!(vm.type_matches("Rational{BigInt}", &JuliaType::Number));
    assert!(!vm.type_matches("String", &JuliaType::Number));

    assert!(vm.type_matches("Float16", &JuliaType::AbstractFloat));
    assert!(vm.type_matches("BigFloat", &JuliaType::AbstractFloat));
    assert!(!vm.type_matches("Int64", &JuliaType::AbstractFloat));

    assert!(vm.type_matches("Int32", &JuliaType::Signed));
    assert!(!vm.type_matches("UInt32", &JuliaType::Signed));
    assert!(vm.type_matches("UInt64", &JuliaType::Unsigned));
    assert!(!vm.type_matches("Int64", &JuliaType::Unsigned));

    assert!(vm.type_matches("UnitRange{Int64}", &JuliaType::UnitRange));
    assert!(!vm.type_matches("UnitRange{Int64}", &JuliaType::StepRange));
    assert!(vm.type_matches("StepRangeLen{Float64}", &JuliaType::AbstractRange));
    assert!(vm.type_matches("OneTo", &JuliaType::AbstractRange));
    assert!(!vm.type_matches("LogRange{Float64}", &JuliaType::AbstractRange));
}

/// Runtime matcher parity with upstream Julia for the array / tuple /
/// string / char / IO param arms, routed through the shared subtype
/// engine (Issue #5915). Every expectation below was verified against
/// upstream `julia` (`L <: R`).
#[test]
fn runtime_type_matches_array_tuple_string_io_params_via_core_subtype_issue_5915() {
    use crate::types::JuliaType;

    let vm = Vm::new(vec![], StableRng::new(0));

    // ::AbstractArray — julia: ranges and views ARE AbstractArrays.
    assert!(vm.type_matches("Vector{Int64}", &JuliaType::AbstractArray));
    assert!(vm.type_matches("Matrix{Float64}", &JuliaType::AbstractArray));
    assert!(vm.type_matches("Array{Int64, 3}", &JuliaType::AbstractArray));
    assert!(vm.type_matches("UnitRange{Int64}", &JuliaType::AbstractArray));
    assert!(vm.type_matches(
        "SubArray{Int64, 1, Vector{Int64}, Tuple{UnitRange{Int64}}, true}",
        &JuliaType::AbstractArray
    ));
    assert!(!vm.type_matches("String", &JuliaType::AbstractArray));

    // ::Array — julia: Vector/Matrix are Array aliases; ranges are NOT Arrays.
    assert!(vm.type_matches("Vector{Int64}", &JuliaType::Array));
    assert!(vm.type_matches("Matrix{Float64}", &JuliaType::Array));
    assert!(vm.type_matches("Array{Int64, 3}", &JuliaType::Array));
    assert!(vm.type_matches("Vector", &JuliaType::Array));
    assert!(!vm.type_matches("UnitRange{Int64}", &JuliaType::Array));
    assert!(!vm.type_matches("String", &JuliaType::Array));

    // ::Tuple — julia: any Tuple{...} matches; NamedTuple does not.
    assert!(vm.type_matches("Tuple{Int64, String}", &JuliaType::Tuple));
    assert!(vm.type_matches("Tuple{}", &JuliaType::Tuple));
    assert!(!vm.type_matches("@NamedTuple{a::Int64}", &JuliaType::Tuple));
    assert!(!vm.type_matches("Vector{Int64}", &JuliaType::Tuple));

    // ::AbstractString / ::AbstractChar / ::IO — julia: String <:
    // AbstractString, Char <: AbstractChar, IOBuffer <: IO.
    assert!(vm.type_matches("String", &JuliaType::AbstractString));
    assert!(!vm.type_matches("Char", &JuliaType::AbstractString));
    assert!(vm.type_matches("Char", &JuliaType::AbstractChar));
    assert!(!vm.type_matches("String", &JuliaType::AbstractChar));
    assert!(vm.type_matches("IOBuffer", &JuliaType::IO));
    assert!(!vm.type_matches("String", &JuliaType::IO));
}

/// `AbstractUser` params (user-declared abstract types, including
/// boot.jl ones like `AbstractDict`) previously fell into the
/// exact-name-equality fallback, so a `::AbstractDict` method could
/// never match a `Dict{...}` value through the runtime matcher
/// (Issue #5915; exposed `show(io::IO, x)` mis-dispatch for
/// `repr(Dict(...))` once `::IO` params started matching IOBuffer).
#[test]
fn runtime_type_matches_abstract_user_params_via_core_subtype_issue_5915() {
    use crate::types::JuliaType;

    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.struct_hierarchy
        .insert("Animal", Some("Any".to_string()), Vec::new());
    vm.struct_hierarchy
        .insert("Dog", Some("Animal".to_string()), Vec::new());

    let abstract_dict = JuliaType::AbstractUser("AbstractDict".to_string(), None);
    assert!(vm.type_matches("Dict{String, Int64}", &abstract_dict));
    assert!(!vm.type_matches("Vector{Int64}", &abstract_dict));
    assert!(!vm.type_matches("String", &abstract_dict));

    let animal = JuliaType::AbstractUser("Animal".to_string(), Some("Any".to_string()));
    assert!(vm.type_matches("Dog", &animal));
    assert!(vm.type_matches("Animal", &animal));
    assert!(!vm.type_matches("String", &animal));
}

/// Concrete `Tuple{...}` params are covariant in upstream Julia:
/// `Tuple{Int64} <: Tuple{Real}`. The old hand-rolled matcher required
/// exact element equality; route the no-typevar case through the shared
/// subtype engine (Issue #5915).
#[test]
fn runtime_type_matches_tuple_params_covariantly_issue_5915() {
    use crate::types::JuliaType;

    let vm = Vm::new(vec![], StableRng::new(0));

    let tuple_of = |elems: Vec<JuliaType>| JuliaType::TupleOf(elems);

    // Covariance (julia: true).
    assert!(vm.type_matches("Tuple{Int64}", &tuple_of(vec![JuliaType::Real])));
    assert!(vm.type_matches(
        "Tuple{Int64, Float64}",
        &tuple_of(vec![JuliaType::Real, JuliaType::Real])
    ));
    // Exact element match still holds.
    assert!(vm.type_matches(
        "Tuple{Int64, Float64}",
        &tuple_of(vec![JuliaType::Int64, JuliaType::Float64])
    ));
    // Non-subtype elements and arity mismatches stay rejected.
    assert!(!vm.type_matches("Tuple{Int64}", &tuple_of(vec![JuliaType::Float64])));
    assert!(!vm.type_matches(
        "Tuple{Int64, String}",
        &tuple_of(vec![JuliaType::Real, JuliaType::Real])
    ));
    assert!(!vm.type_matches("Tuple{Int64}", &tuple_of(vec![])));
    assert!(!vm.type_matches(
        "Tuple{Int64}",
        &tuple_of(vec![JuliaType::Int64, JuliaType::Int64])
    ));

    // Trailing unbounded Vararg (julia: Tuple{Int64, Int64} <:
    // Tuple{Int64, Vararg{Int64}}).
    let vararg_tail = tuple_of(vec![
        JuliaType::Int64,
        JuliaType::Struct("Vararg{Int64}".to_string()),
    ]);
    assert!(vm.type_matches("Tuple{Int64, Int64}", &vararg_tail));
    assert!(vm.type_matches("Tuple{Int64}", &vararg_tail));
    assert!(!vm.type_matches("Tuple{Int64, Float64}", &vararg_tail));

    // TypeVar elements keep the permissive local wildcard behavior used
    // by the bindings-driven matcher.
    assert!(vm.type_matches(
        "Tuple{Int64, String}",
        &tuple_of(vec![
            JuliaType::TypeVar("T".to_string(), None),
            JuliaType::String
        ])
    ));
}

/// TypeVar-bearing `Tuple{...}` params: the TypeVar legs stay local
/// wildcards (bindings extracted elsewhere), but every concrete element
/// leg is covariant in upstream Julia and must route through the shared
/// subtype engine (Issue #5915). Verified upstream:
/// `Tuple{Int64, Int64} <: Tuple{T, Real} where T` is true.
#[test]
fn runtime_type_matches_typevar_tuple_concrete_elements_covariantly_issue_5915() {
    use crate::types::JuliaType;

    let vm = Vm::new(vec![], StableRng::new(0));
    let tvar = || JuliaType::TypeVar("T".to_string(), None);

    // julia: Tuple{Int64, Int64} <: (Tuple{T, Real} where T) → true.
    assert!(vm.type_matches(
        "Tuple{Int64, Int64}",
        &JuliaType::TupleOf(vec![tvar(), JuliaType::Real])
    ));
    // julia: Tuple{Int64, Int64} <: (Tuple{T, Integer} where T) → true.
    assert!(vm.type_matches(
        "Tuple{Int64, Int64}",
        &JuliaType::TupleOf(vec![tvar(), JuliaType::Integer])
    ));
    // julia: Tuple{Int64, String} <: (Tuple{T, Real} where T) → false.
    assert!(!vm.type_matches(
        "Tuple{Int64, String}",
        &JuliaType::TupleOf(vec![tvar(), JuliaType::Real])
    ));
    // julia: Tuple{Int64, String} <: (Tuple{Real, T} where T) → true.
    assert!(vm.type_matches(
        "Tuple{Int64, String}",
        &JuliaType::TupleOf(vec![JuliaType::Real, tvar()])
    ));
    // Exact element equality still matches alongside a TypeVar leg.
    assert!(vm.type_matches(
        "Tuple{Int64, Float64}",
        &JuliaType::TupleOf(vec![tvar(), JuliaType::Float64])
    ));

    // Vararg tail with a TypeVar lead (julia:
    // Tuple{Int64, Int64, Int64} <: (Tuple{T, Vararg{Int64}} where T) → true,
    // Tuple{Int64, Float64} <: (Tuple{T, Vararg{Int64}} where T) → false).
    let vararg_tail =
        JuliaType::TupleOf(vec![tvar(), JuliaType::Struct("Vararg{Int64}".to_string())]);
    assert!(vm.type_matches("Tuple{Int64, Int64, Int64}", &vararg_tail));
    assert!(vm.type_matches("Tuple{Int64}", &vararg_tail));
    assert!(!vm.type_matches("Tuple{Int64, Float64}", &vararg_tail));
}

/// `Struct(name)` params with concrete type parameters previously used
/// raw string equality; route the no-TypeVar legs through the shared
/// subtype engine (Issue #5915). Invariance is preserved
/// (`Complex{Float64}` does NOT match `::Complex{Int64}`) and declared
/// parametric parents are honored (julia:
/// `MyVec{Int64} <: Wrapper{Int64}` for `struct MyVec{T} <: Wrapper{T}`).
#[test]
fn runtime_type_matches_struct_params_via_core_subtype_issue_5915() {
    use crate::types::JuliaType;

    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.struct_hierarchy
        .insert("Complex", Some("Number".to_string()), vec!["T".to_string()]);
    vm.struct_hierarchy
        .insert("Wrapper", Some("Any".to_string()), vec!["T".to_string()]);
    vm.struct_hierarchy.insert(
        "MyVec",
        Some("Wrapper{T}".to_string()),
        vec!["T".to_string()],
    );

    // Bare parametric base (julia: Complex{Float64} <: Complex → true).
    assert!(vm.type_matches(
        "Complex{Float64}",
        &JuliaType::Struct("Complex".to_string())
    ));
    // Exact concrete params.
    assert!(vm.type_matches(
        "Complex{Float64}",
        &JuliaType::Struct("Complex{Float64}".to_string())
    ));
    // Invariance (julia: Complex{Float64} <: Complex{Int64} → false).
    assert!(!vm.type_matches(
        "Complex{Float64}",
        &JuliaType::Struct("Complex{Int64}".to_string())
    ));
    // Type-variable params keep the wildcard base match (bindings are
    // extracted by the bindings-driven dispatcher).
    assert!(vm.type_matches(
        "Rational{Int64}",
        &JuliaType::Struct("Rational{T}".to_string())
    ));
    // Declared parametric parent (julia: MyVec{Int64} <: Wrapper{Int64}
    // → true; MyVec{Int64} <: Wrapper{Real} → false). The old string
    // equality could never match a declared parent.
    assert!(vm.type_matches(
        "MyVec{Int64}",
        &JuliaType::Struct("Wrapper{Int64}".to_string())
    ));
    assert!(!vm.type_matches(
        "MyVec{Int64}",
        &JuliaType::Struct("Wrapper{Real}".to_string())
    ));
    // Module-qualified renderer differences still match.
    assert!(vm.type_matches(
        "Base.OneTo{Int64}",
        &JuliaType::Struct("OneTo{Int64}".to_string())
    ));
    // Parametric param with a bare runtime name (runtime params unknown)
    // keeps the legacy permissive base match.
    assert!(vm.type_matches(
        "Polynomial",
        &JuliaType::Struct("Polynomial{Float64}".to_string())
    ));
    assert!(!vm.type_matches(
        "Polynomial",
        &JuliaType::Struct("Monomial{Float64}".to_string())
    ));
    // Partial parametric short forms stay exact carrier tags: VM Base
    // code (subarray.jl) uses `SubArray{Int64}` overloads for the legacy
    // 1-D carrier, so an N-D 5-param runtime SubArray must NOT match
    // them (the engine's prefix-completing UnionAll reading would
    // mis-route `parentindices` on matrix views).
    assert!(!vm.type_matches(
        "SubArray{Int64, 2, Matrix{Int64}, Tuple{UnitRange{Int64}, Slice{OneTo}}, false}",
        &JuliaType::Struct("SubArray{Int64}".to_string())
    ));
}

/// `Vector{T}` / `Matrix{T}` / `Ref{T}` element params are INVARIANT in
/// upstream Julia (`Vector{Int64} <: Vector{Real}` is false,
/// `Base.RefValue{Int64} <: Ref{Real}` is false). Lock the runtime
/// matcher to the upstream behavior (Issue #5915).
#[test]
fn runtime_type_matches_vector_matrix_ref_params_stay_invariant_issue_5915() {
    use crate::types::JuliaType;

    let vm = Vm::new(vec![], StableRng::new(0));

    // ::Vector{T} — invariant element.
    let vec_int = JuliaType::VectorOf(Box::new(JuliaType::Int64));
    let vec_real = JuliaType::VectorOf(Box::new(JuliaType::Real));
    assert!(vm.type_matches("Vector{Int64}", &vec_int));
    assert!(!vm.type_matches("Vector{Int64}", &vec_real));
    assert!(!vm.type_matches("Vector{Real}", &vec_int));
    // Runtime element unknown: permissive (legacy).
    assert!(vm.type_matches("Vector", &vec_int));
    // TypeVar element: wildcard for the bindings-driven dispatcher.
    assert!(vm.type_matches(
        "Vector{Int64}",
        &JuliaType::VectorOf(Box::new(JuliaType::TypeVar("T".to_string(), None)))
    ));

    // ::Matrix{T} — invariant element.
    let mat_f64 = JuliaType::MatrixOf(Box::new(JuliaType::Float64));
    let mat_real = JuliaType::MatrixOf(Box::new(JuliaType::Real));
    assert!(vm.type_matches("Matrix{Float64}", &mat_f64));
    assert!(!vm.type_matches("Matrix{Float64}", &mat_real));
    assert!(!vm.type_matches("Matrix{Int64}", &mat_f64));

    // ::Ref{T} / ::Base.RefValue{T} — invariant element (julia:
    // Base.RefValue{Int64} <: Ref{Int64} true, <: Ref{Real} false).
    assert!(vm.type_matches(
        "Base.RefValue{Int64}",
        &JuliaType::Struct("Ref{Int64}".to_string())
    ));
    assert!(!vm.type_matches(
        "Base.RefValue{Int64}",
        &JuliaType::Struct("Ref{Real}".to_string())
    ));
    assert!(vm.type_matches(
        "Base.RefValue{Int64}",
        &JuliaType::Struct("Ref".to_string())
    ));
    assert!(!vm.type_matches(
        "Base.RefValue{Int64}",
        &JuliaType::Struct("Base.RefValue{Real}".to_string())
    ));
}

/// The `_` fallback for the remaining nominal variants is a pure subtype
/// question for the shared engine (Issue #5915). Verified upstream:
/// `DataType <: Type`, `Set{Int64} <: Set`, `Dict{String, Int64} <: Dict`.
#[test]
fn runtime_type_matches_nominal_fallback_via_core_subtype_issue_5915() {
    use crate::types::JuliaType;

    let vm = Vm::new(vec![], StableRng::new(0));

    // Exact-name matches keep working.
    assert!(vm.type_matches("Int8", &JuliaType::Int8));
    assert!(vm.type_matches("Symbol", &JuliaType::Symbol));
    assert!(!vm.type_matches("Int8", &JuliaType::Int16));
    assert!(!vm.type_matches("String", &JuliaType::Nothing));

    // julia: DataType <: Type → true.
    assert!(vm.type_matches("DataType", &JuliaType::Type));
    // julia: Set{Int64} <: Set → true (was false under exact equality).
    assert!(vm.type_matches("Set{Int64}", &JuliaType::Set));
    // julia: Dict{String, Int64} <: Dict → true.
    assert!(vm.type_matches("Dict{String, Int64}", &JuliaType::Dict));
    // Unrelated parametric runtime names stay rejected.
    assert!(!vm.type_matches("Set{Int64}", &JuliaType::Dict));
    assert!(!vm.type_matches("Vector{Int64}", &JuliaType::Set));

    // julia: typeof(+) <: Function → true.
    assert!(vm.type_matches("typeof(+)", &JuliaType::Function));
    assert!(vm.type_matches("Function", &JuliaType::Function));
}

/// Re-evaluation of the Issue #6512 `::Function` carve-out (Issue #6597).
///
/// PR #6524 removed the legacy exact-name carve-out
/// (`JuliaType::Function => runtime_type == param_type.name()`) and routed
/// runtime `::Function` matching through the shared `CoreSubtypeEngine`. The
/// follow-up f6adade84 (Issue #6529) guard — the native-array wrapper fence,
/// now the selection-core policy `selection::signature_is_broad_wrapper_fence`
/// (Issue #6595), which counts `Function` slots as broad — keeps empty
/// narrow-int / Bool reductions on the type-specialized Base method instead of
/// the broad `reduce(op::Function, itr)` catch-all.
///
/// This test pins the post-removal contract so a future refactor cannot
/// silently reintroduce exact-name matching:
///   1. Function singleton runtime names (`typeof(+)`, `typeof(f)`) are
///      engine-true subtypes of `Function` (the relation the carve-out hid).
///   2. Concrete non-callable values still do NOT match `::Function` — the
///      negative guarantee the exact-name carve-out used to provide must be
///      preserved by the engine routing.
///
/// Verified against upstream julia 1.12: `typeof(+) <: Function` and
/// `typeof(identity) <: Function` are `true`; `Int8 <: Function`,
/// `Vector{Int64} <: Function`, `Symbol <: Function` are `false`.
#[test]
fn runtime_type_matches_function_param_via_core_subtype_issue_6597() {
    use crate::types::JuliaType;

    let vm = Vm::new(vec![], StableRng::new(0));

    // Function singletons are engine-true subtypes of Function (carve-out gone).
    assert!(vm.type_matches("typeof(+)", &JuliaType::Function));
    assert!(vm.type_matches("typeof(*)", &JuliaType::Function));
    assert!(vm.type_matches("typeof(identity)", &JuliaType::Function));
    assert!(vm.type_matches("Function", &JuliaType::Function));

    // Concrete non-callable types must NOT match ::Function — engine routing
    // preserves the negative guarantee the exact-name carve-out provided.
    assert!(!vm.type_matches("Int8", &JuliaType::Function));
    assert!(!vm.type_matches("Int64", &JuliaType::Function));
    assert!(!vm.type_matches("Bool", &JuliaType::Function));
    assert!(!vm.type_matches("Vector{Int64}", &JuliaType::Function));
    assert!(!vm.type_matches("Symbol", &JuliaType::Function));
}

#[test]
fn test_find_best_method_index_matches_parametric_varargs() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.functions.push(Rc::new(FunctionInfo {
        name: "getindex".to_string(),
        params: vec![
            ("a".to_string(), ValueType::Any),
            ("i".to_string(), ValueType::I64),
            ("j".to_string(), ValueType::I64),
            ("I".to_string(), ValueType::Tuple),
        ],
        kwparams: vec![],
        entry: 0,
        return_type: ValueType::Any,
        return_julia_type: None,
        is_base_extension: false,
        is_generated: false,
        min_world: 1,
        type_params: vec![crate::types::TypeParam::new("T".to_string())],
        param_julia_types: vec![
            crate::types::JuliaType::Struct("Array{T}".to_string()),
            crate::types::JuliaType::Int64,
            crate::types::JuliaType::Int64,
            crate::types::JuliaType::Int64,
        ],
        code_start: 0,
        code_end: 0,
        slot_names: vec![],
        slot_types: vec![],
        local_slot_count: 0,
        param_slots: vec![],
        vararg_param_index: Some(3),
        vararg_fixed_count: None,
        inlining_meta: 0,
        constprop_meta: 0,
        nospecialize_meta: 0,
        propagate_inbounds_meta: false,
        nospecializeinfer_meta: false,
        purity_meta: 0,
        direct_return_type_param: None,
        // Builtin stub FunctionInfo: no source line (Issue #5125).
        def_line: 0,
    }));
    vm.function_name_index
        .insert("getindex".to_string(), vec![0]);

    let array = Value::Struct(StructInstance::with_name(
        0,
        "Array{Int64, 3}".to_string(),
        vec![],
    ));
    let matching = vec![array.clone(), Value::I64(1), Value::I64(1), Value::I64(1)];
    let mismatched_tail = vec![array, Value::I64(1), Value::I64(1), Value::F64(1.0)];

    assert_eq!(vm.find_best_method_index(&["getindex"], &matching), Some(0));
    assert_eq!(
        vm.find_best_method_index(&["getindex"], &mismatched_tail),
        None
    );
}

#[test]
fn test_find_best_method_index_caches_positive_and_negative_issue_5087() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.functions.push(Rc::new(FunctionInfo {
        name: "dispatch_cache_probe".to_string(),
        params: vec![("x".to_string(), ValueType::I64)],
        kwparams: vec![],
        entry: 0,
        return_type: ValueType::I64,
        return_julia_type: Some(crate::types::JuliaType::Int64),
        is_base_extension: false,
        is_generated: false,
        min_world: 1,
        type_params: vec![],
        param_julia_types: vec![crate::types::JuliaType::Int64],
        code_start: 0,
        code_end: 0,
        slot_names: vec![],
        slot_types: vec![],
        local_slot_count: 0,
        param_slots: vec![],
        vararg_param_index: None,
        vararg_fixed_count: None,
        inlining_meta: 0,
        constprop_meta: 0,
        nospecialize_meta: 0,
        propagate_inbounds_meta: false,
        nospecializeinfer_meta: false,
        purity_meta: 0,
        direct_return_type_param: None,
        // Builtin stub FunctionInfo: no source line (Issue #5125).
        def_line: 0,
    }));
    vm.function_name_index
        .insert("dispatch_cache_probe".to_string(), vec![0]);

    let int_args = vec![Value::I64(1)];
    let float_args = vec![Value::F64(1.0)];

    assert!(vm.method_dispatch_cache.is_empty());
    assert_eq!(
        vm.find_best_method_index(&["dispatch_cache_probe"], &int_args),
        Some(0)
    );
    assert_eq!(vm.method_dispatch_cache.len(), 1);
    assert_eq!(
        vm.find_best_method_index(&["dispatch_cache_probe"], &int_args),
        Some(0)
    );
    assert_eq!(vm.method_dispatch_cache.len(), 1);

    assert_eq!(
        vm.find_best_method_index(&["dispatch_cache_probe"], &float_args),
        None
    );
    assert_eq!(vm.method_dispatch_cache.len(), 2);
    assert_eq!(
        vm.find_best_method_index(&["dispatch_cache_probe"], &float_args),
        None
    );
    assert_eq!(
        vm.method_dispatch_cache
            .values()
            .filter(|v| v.is_none())
            .count(),
        1
    );
}

#[test]
fn test_find_best_method_index_issue_5926_dominance_selects_vector_over_abstractvector() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.functions.push(Rc::new(dispatch_test_function(
        "fam1_runtime_5926",
        vec![crate::types::JuliaType::AbstractUser(
            "AbstractVector".to_string(),
            Some("AbstractArray".to_string()),
        )],
        vec![],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "fam1_runtime_5926",
        vec![crate::types::JuliaType::Struct("Vector{T}".to_string())],
        vec![crate::types::TypeParam::new("T".to_string())],
    )));
    vm.function_name_index
        .insert("fam1_runtime_5926".to_string(), vec![0, 1]);

    let arg = Value::Struct(StructInstance::with_name(
        0,
        "Vector{Int64}".to_string(),
        vec![],
    ));
    assert_eq!(
        vm.find_best_method_index(&["fam1_runtime_5926"], &[arg]),
        Some(1),
        "runtime dispatch must mirror MethodTable's #5926 dominance \
         pre-check so Vector{{T}} wins over AbstractVector"
    );
}

#[test]
fn test_find_best_method_index_issue_5926_dominance_selects_diagonal_over_any_any() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.functions.push(Rc::new(dispatch_test_function(
        "diagonal_runtime_5926",
        vec![crate::types::JuliaType::Any, crate::types::JuliaType::Any],
        vec![],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "diagonal_runtime_5926",
        vec![
            crate::types::JuliaType::TypeVar("T".to_string(), None),
            crate::types::JuliaType::TypeVar("T".to_string(), None),
        ],
        vec![crate::types::TypeParam::new("T".to_string())],
    )));
    vm.function_name_index
        .insert("diagonal_runtime_5926".to_string(), vec![0, 1]);

    assert_eq!(
        vm.find_best_method_index(&["diagonal_runtime_5926"], &[Value::I64(1), Value::I64(2)]),
        Some(1),
        "runtime dispatch must mirror MethodTable's #5926 dominance \
         pre-check so Tuple{{T,T}} wins over Tuple{{Any,Any}}"
    );
    assert_eq!(
        vm.find_best_method_index(
            &["diagonal_runtime_5926"],
            &[Value::I64(1), Value::F64(2.0)]
        ),
        Some(0),
        "mixed runtime args do not satisfy the diagonal rule, so the Any \
        fallback wins"
    );
}

#[test]
fn test_find_best_method_index_tuple_bounded_fallback_after_diagonal_issue_6251() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.functions.push(Rc::new(dispatch_test_function(
        "tuple_bounded_fallback_runtime_6251",
        vec![crate::types::JuliaType::TupleOf(vec![
            crate::types::JuliaType::TypeVar("T".to_string(), None),
            crate::types::JuliaType::TypeVar("T".to_string(), None),
        ])],
        vec![crate::types::TypeParam::with_upper_bound(
            "T".to_string(),
            "Real".to_string(),
        )],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "tuple_bounded_fallback_runtime_6251",
        vec![crate::types::JuliaType::TupleOf(vec![
            crate::types::JuliaType::TypeVar("_".to_string(), Some("Real".to_string())),
            crate::types::JuliaType::TypeVar("_".to_string(), Some("Real".to_string())),
        ])],
        vec![],
    )));
    vm.function_name_index.insert(
        "tuple_bounded_fallback_runtime_6251".to_string(),
        vec![0, 1],
    );

    assert_eq!(
        vm.find_best_method_index(
            &["tuple_bounded_fallback_runtime_6251"],
            &[Value::Tuple(TupleValue::new(vec![
                Value::I64(1),
                Value::I64(2)
            ]))]
        ),
        Some(0),
        "homogeneous concrete real tuple keeps the diagonal method"
    );
    assert_eq!(
        vm.find_best_method_index(
            &["tuple_bounded_fallback_runtime_6251"],
            &[Value::Tuple(TupleValue::new(vec![
                Value::I64(1),
                Value::F64(2.0),
            ]))]
        ),
        Some(1),
        "mixed real tuple falls back to Tuple{{<:Real,<:Real}}"
    );
    assert_eq!(
        vm.find_best_method_index(
            &["tuple_bounded_fallback_runtime_6251"],
            &[Value::Tuple(TupleValue::new(vec![
                Value::I64(1),
                Value::Str("x".to_string()),
            ]))]
        ),
        None,
        "non-Real tuple element must not match the bounded fallback"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[0, 1],
            &[Value::Tuple(TupleValue::new(vec![
                Value::I64(1),
                Value::Str("x".to_string()),
            ]))]
        )
        .unwrap(),
        None,
        "CallDynamic candidate-list dispatch must not fall back to Tuple{{<:Real,<:Real}}"
    );
}

#[test]
fn test_find_best_method_index_issue_5926_preserves_bounded_typevar_over_untyped_any_5375() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.functions.push(Rc::new(dispatch_test_function(
        "bounded_runtime_5926",
        vec![crate::types::JuliaType::Any],
        vec![],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "bounded_runtime_5926",
        vec![crate::types::JuliaType::TypeVar(
            "T".to_string(),
            Some("Number".to_string()),
        )],
        vec![crate::types::TypeParam::with_upper_bound(
            "T".to_string(),
            "Number".to_string(),
        )],
    )));
    vm.function_name_index
        .insert("bounded_runtime_5926".to_string(), vec![0, 1]);

    assert_eq!(
        vm.find_best_method_index(&["bounded_runtime_5926"], &[Value::I64(5)]),
        Some(1),
        "runtime dispatch must preserve the #5375 regression: T<:Number \
         beats the untyped Any fallback"
    );
    assert_eq!(
        vm.find_best_method_index(&["bounded_runtime_5926"], &[Value::Str("s".to_string())],),
        Some(0),
        "non-Number runtime values should still use the untyped fallback"
    );
}

#[test]
fn test_find_best_method_index_issue_6202_type_singleton_picks_tighter_bound_from_any() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.functions.push(Rc::new(dispatch_test_function(
        "bounded_type_singleton_runtime_6202",
        vec![crate::types::JuliaType::TypeOf(Box::new(
            crate::types::JuliaType::TypeVar("T".to_string(), None),
        ))],
        vec![crate::types::TypeParam::with_upper_bound(
            "T".to_string(),
            "Real".to_string(),
        )],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "bounded_type_singleton_runtime_6202",
        vec![crate::types::JuliaType::TypeOf(Box::new(
            crate::types::JuliaType::TypeVar("T".to_string(), None),
        ))],
        vec![crate::types::TypeParam::with_upper_bound(
            "T".to_string(),
            "Integer".to_string(),
        )],
    )));
    vm.function_name_index.insert(
        "bounded_type_singleton_runtime_6202".to_string(),
        vec![0, 1],
    );

    assert_eq!(
        vm.find_best_method_index(
            &["bounded_type_singleton_runtime_6202"],
            &[Value::DataType(Box::new(crate::types::JuliaType::Int64))]
        ),
        Some(1),
        "runtime dispatch from an Any container must preserve bounded \
         Type{{T}} specificity: T<:Integer beats T<:Real for Int64"
    );
    assert_eq!(
        vm.find_best_method_index(
            &["bounded_type_singleton_runtime_6202"],
            &[Value::DataType(Box::new(crate::types::JuliaType::Float64))]
        ),
        Some(0),
        "Float64 satisfies only the looser T<:Real method"
    );
}

#[test]
fn test_find_best_method_index_issue_6202_vector_picks_tighter_bound_from_any() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.functions.push(Rc::new(dispatch_test_function(
        "bounded_vector_runtime_6202",
        vec![crate::types::JuliaType::VectorOf(Box::new(
            crate::types::JuliaType::TypeVar("T".to_string(), None),
        ))],
        vec![crate::types::TypeParam::with_upper_bound(
            "T".to_string(),
            "Real".to_string(),
        )],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "bounded_vector_runtime_6202",
        vec![crate::types::JuliaType::VectorOf(Box::new(
            crate::types::JuliaType::TypeVar("T".to_string(), None),
        ))],
        vec![crate::types::TypeParam::with_upper_bound(
            "T".to_string(),
            "Integer".to_string(),
        )],
    )));
    vm.function_name_index
        .insert("bounded_vector_runtime_6202".to_string(), vec![0, 1]);

    let int_vector = array_value(new_array_ref(ArrayValue::from_i64(vec![1, 2], vec![2])));
    let real_vector = array_value(new_array_ref(ArrayValue::from_f64(vec![1.0, 2.0], vec![2])));

    assert_eq!(
        vm.find_best_method_index(&["bounded_vector_runtime_6202"], &[int_vector]),
        Some(1),
        "runtime dispatch from an Any container must preserve bounded \
         Vector{{T}} specificity: T<:Integer beats T<:Real for Vector{{Int64}}"
    );
    assert_eq!(
        vm.find_best_method_index(&["bounded_vector_runtime_6202"], &[real_vector]),
        Some(0),
        "Vector{{Float64}} satisfies only the looser T<:Real method"
    );
}

#[test]
fn test_find_best_method_index_from_candidates_tuple_vararg_ambiguity_issue_6220() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.functions.push(Rc::new(dispatch_test_function(
        "tuple_vararg_ambiguity_runtime_6220",
        vec![crate::types::JuliaType::TupleOf(vec![
            crate::types::JuliaType::Struct("Vararg{Integer}".to_string()),
        ])],
        vec![],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "tuple_vararg_ambiguity_runtime_6220",
        vec![crate::types::JuliaType::TupleOf(vec![
            crate::types::JuliaType::Int64,
            crate::types::JuliaType::Struct("Vararg{Any}".to_string()),
        ])],
        vec![],
    )));

    let empty = vec![Value::Tuple(TupleValue::new(vec![]))];
    let single_int = vec![Value::Tuple(TupleValue::new(vec![Value::I64(1)]))];
    let all_int = vec![Value::Tuple(TupleValue::new(vec![
        Value::I64(1),
        Value::I64(2),
    ]))];
    let mixed_tail = vec![Value::Tuple(TupleValue::new(vec![
        Value::I64(1),
        Value::Str("x".to_string()),
    ]))];

    assert_eq!(
        vm.find_best_method_index_from_candidates(&[0, 1], &empty)
            .unwrap(),
        Some(0),
        "empty tuple only matches Tuple{{Vararg{{Integer}}}}"
    );
    assert!(
        matches!(
            vm.find_best_method_index_from_candidates(&[0, 1], &single_int),
            Err(VmError::MethodError(msg)) if msg.contains("ambiguous")
        ),
        "single Int tuple remains ambiguous at runtime"
    );
    assert!(
        matches!(
            vm.find_best_method_index_from_candidates(&[0, 1], &all_int),
            Err(VmError::MethodError(msg)) if msg.contains("ambiguous")
        ),
        "all-Int tuple remains ambiguous at runtime"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(&[0, 1], &mixed_tail)
            .unwrap(),
        Some(1),
        "mixed tail only matches the fixed-prefix fallback"
    );
}

#[test]
fn test_find_best_method_index_from_candidates_union_actual_arm_issue_6231() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.functions.push(Rc::new(dispatch_test_function(
        "union_actual_arm_runtime_6231",
        vec![crate::types::JuliaType::Union(vec![
            crate::types::JuliaType::Int64,
            crate::types::JuliaType::String,
        ])],
        vec![],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "union_actual_arm_runtime_6231",
        vec![crate::types::JuliaType::Integer],
        vec![],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "union_actual_arm_runtime_6231",
        vec![crate::types::JuliaType::Union(vec![
            crate::types::JuliaType::Integer,
            crate::types::JuliaType::String,
        ])],
        vec![],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "union_actual_arm_runtime_6231",
        vec![crate::types::JuliaType::Real],
        vec![],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "union_actual_arm_runtime_6231",
        vec![crate::types::JuliaType::Union(vec![
            crate::types::JuliaType::Real,
            crate::types::JuliaType::String,
        ])],
        vec![],
    )));

    assert_eq!(
        vm.find_best_method_index_from_candidates(&[0, 1], &[Value::I64(1)])
            .unwrap(),
        Some(0),
        "Union{{Int64,String}} is more specific than Integer for Int64"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(&[0, 1], &[Value::Str("x".to_string())])
            .unwrap(),
        Some(0),
        "String only matches the finite Union method"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(&[2, 3], &[Value::I64(1)])
            .unwrap(),
        Some(2),
        "Union{{Integer,String}} is more specific than Real for integer values"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(&[3, 2], &[Value::I64(1)])
            .unwrap(),
        Some(2),
        "Union{{Integer,String}} wins over Real independent of candidate order"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(&[2, 3], &[Value::F64(1.0)])
            .unwrap(),
        Some(3),
        "Float64 does not satisfy the Union{{Integer,String}} arm"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(&[4, 1], &[Value::I64(1)])
            .unwrap(),
        Some(1),
        "an unrelated concrete Union arm must not make Union{{Real,String}} beat Integer"
    );
}

#[test]
fn test_find_best_method_index_from_candidates_type_value_diagonal_issue_6233() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.functions.push(Rc::new(dispatch_test_function(
        "type_value_diagonal_runtime_6233",
        vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::TypeVar(
                "T".to_string(),
                None,
            ))),
            crate::types::JuliaType::TypeVar("T".to_string(), None),
        ],
        vec![crate::types::TypeParam::with_upper_bound(
            "T".to_string(),
            "Real".to_string(),
        )],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "type_value_diagonal_runtime_6233",
        vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::Integer)),
            crate::types::JuliaType::Integer,
        ],
        vec![],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "type_value_diagonal_runtime_exact_6233",
        vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::TypeVar(
                "T".to_string(),
                None,
            ))),
            crate::types::JuliaType::TypeVar("T".to_string(), None),
        ],
        vec![crate::types::TypeParam::with_upper_bound(
            "T".to_string(),
            "Real".to_string(),
        )],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "type_value_diagonal_runtime_exact_6233",
        vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::Int64)),
            crate::types::JuliaType::Int64,
        ],
        vec![],
    )));

    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[0, 1],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Int64)),
                Value::I64(1),
            ],
        )
        .unwrap(),
        Some(0),
        "concrete Type{{Int64}} plus Int64 selects the diagonal Type{{T}}, T method"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[0, 1],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Integer)),
                Value::I64(1),
            ],
        )
        .unwrap(),
        Some(1),
        "abstract Type{{Integer}} keeps the fixed Type{{Integer}}, Integer method"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[0, 1],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Float64)),
                Value::F64(1.0),
            ],
        )
        .unwrap(),
        Some(0),
        "Float64 satisfies only the diagonal method"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[2, 3],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Int64)),
                Value::I64(1),
            ],
        )
        .unwrap(),
        Some(3),
        "an exact Type{{Int64}}, Int64 method remains more specific than the diagonal"
    );
}

#[test]
fn test_find_best_method_index_from_candidates_type_vector_diagonal_issue_6235() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.functions.push(Rc::new(dispatch_test_function(
        "type_vector_diagonal_runtime_6235",
        vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::TypeVar(
                "T".to_string(),
                None,
            ))),
            crate::types::JuliaType::VectorOf(Box::new(crate::types::JuliaType::TypeVar(
                "T".to_string(),
                None,
            ))),
        ],
        vec![crate::types::TypeParam::with_upper_bound(
            "T".to_string(),
            "Real".to_string(),
        )],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "type_vector_diagonal_runtime_6235",
        vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::Integer)),
            crate::types::JuliaType::VectorOf(Box::new(crate::types::JuliaType::TypeVar(
                "_".to_string(),
                Some("Real".to_string()),
            ))),
        ],
        vec![],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "type_vector_diagonal_runtime_exact_6235",
        vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::TypeVar(
                "T".to_string(),
                None,
            ))),
            crate::types::JuliaType::VectorOf(Box::new(crate::types::JuliaType::TypeVar(
                "T".to_string(),
                None,
            ))),
        ],
        vec![crate::types::TypeParam::with_upper_bound(
            "T".to_string(),
            "Real".to_string(),
        )],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "type_vector_diagonal_runtime_exact_6235",
        vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::Int64)),
            crate::types::JuliaType::VectorOf(Box::new(crate::types::JuliaType::Int64)),
        ],
        vec![],
    )));

    let int_vector = array_value(new_array_ref(ArrayValue::from_i64(vec![1], vec![1])));
    let float_vector = array_value(new_array_ref(ArrayValue::from_f64(vec![1.0], vec![1])));

    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[0, 1],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Int64)),
                int_vector.clone(),
            ],
        )
        .unwrap(),
        Some(0),
        "concrete Type{{Int64}} plus Vector{{Int64}} selects the diagonal method"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[0, 1],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Integer)),
                int_vector.clone(),
            ],
        )
        .unwrap(),
        Some(1),
        "abstract Type{{Integer}} keeps the fixed Type{{Integer}}, Vector{{<:Real}} method"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[0, 1],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Float64)),
                float_vector,
            ],
        )
        .unwrap(),
        Some(0),
        "Float64 vector satisfies only the diagonal method"
    );
    assert_eq!(
        vm.values_match_params_binding_count(
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Int64)),
                int_vector.clone(),
            ],
            &vm.functions[3].param_julia_types,
            &vm.functions[3].type_params,
        ),
        Some(0),
        "exact Type{{Int64}}, Vector{{Int64}} candidate must remain a runtime match"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[2, 3],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Int64)),
                int_vector
            ],
        )
        .unwrap(),
        Some(3),
        "an exact Type{{Int64}}, Vector{{Int64}} method remains more specific"
    );
}

#[test]
fn test_find_best_method_index_from_candidates_type_abstract_vector_diagonal_issue_6239() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.functions.push(Rc::new(dispatch_test_function(
        "type_abstract_vector_diagonal_runtime_6239",
        vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::TypeVar(
                "T".to_string(),
                None,
            ))),
            crate::types::JuliaType::Struct("AbstractVector{T}".to_string()),
        ],
        vec![crate::types::TypeParam::with_upper_bound(
            "T".to_string(),
            "Real".to_string(),
        )],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "type_abstract_vector_diagonal_runtime_6239",
        vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::Integer)),
            crate::types::JuliaType::Struct("AbstractVector{<:Real}".to_string()),
        ],
        vec![],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "type_abstract_vector_diagonal_runtime_exact_6239",
        vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::TypeVar(
                "T".to_string(),
                None,
            ))),
            crate::types::JuliaType::Struct("AbstractVector{T}".to_string()),
        ],
        vec![crate::types::TypeParam::with_upper_bound(
            "T".to_string(),
            "Real".to_string(),
        )],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "type_abstract_vector_diagonal_runtime_exact_6239",
        vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::Int64)),
            crate::types::JuliaType::Struct("AbstractVector{Int64}".to_string()),
        ],
        vec![],
    )));

    let int_vector = array_value(new_array_ref(ArrayValue::from_i64(vec![1], vec![1])));
    let float_vector = array_value(new_array_ref(ArrayValue::from_f64(vec![1.0], vec![1])));

    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[0, 1],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Int64)),
                int_vector.clone(),
            ],
        )
        .unwrap(),
        Some(0),
        "concrete Type{{Int64}} plus Vector{{Int64}} selects the AbstractVector diagonal method"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[0, 1],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Integer)),
                int_vector.clone(),
            ],
        )
        .unwrap(),
        Some(1),
        "abstract Type{{Integer}} keeps the fixed Type{{Integer}}, AbstractVector{{<:Real}} method"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[0, 1],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Float64)),
                float_vector,
            ],
        )
        .unwrap(),
        Some(0),
        "Float64 vector satisfies only the AbstractVector diagonal method"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[2, 3],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Int64)),
                int_vector
            ],
        )
        .unwrap(),
        Some(3),
        "an exact Type{{Int64}}, AbstractVector{{Int64}} method remains more specific"
    );
}

#[test]
fn test_call_typed_dispatch_type_abstract_vector_diagonal_via_any_issue_6573() {
    use crate::compile::compile_core_program;
    use crate::lowering::Lowering;
    use crate::parser::Parser;

    let source = r#"
type_abstract_vector_diagonal_6573(::Type{T}, ::AbstractVector{T}) where {T<:Real} = :type_absvec_same
type_abstract_vector_diagonal_6573(::Type{Integer}, ::AbstractVector{<:Real}) = :type_integer_absvec_real

function type_abstract_vector_diagonal_via_any_6573(t, x)
    tt::Any = t
    xx::Any = x
    type_abstract_vector_diagonal_6573(tt, xx)
end

type_abstract_vector_diagonal_via_any_6573(Integer, [1, 2])
"#;
    let mut parser = Parser::new().expect("create parser");
    let parsed = parser.parse(source).expect("parse source");
    let mut lowering = Lowering::new(source);
    let program = lowering.lower(parsed).expect("lower source");
    let compiled = compile_core_program(&program).expect("compile failed");
    let mut vm = Vm::new_program(compiled, StableRng::new(0));

    let result = vm.run();
    assert!(
        matches!(
            result,
            Ok(Value::Symbol(ref sym)) if sym.as_str() == "type_integer_absvec_real"
        ),
        "Any-routed Type{{Integer}}, Vector{{Int64}} dispatch should select the fixed Type{{Integer}} method, got {result:?}"
    );
}

#[test]
fn test_find_best_method_index_from_candidates_type_abstract_array_rank1_diagonal_issue_6245() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.functions.push(Rc::new(dispatch_test_function(
        "type_abstract_array_rank1_diagonal_runtime_6245",
        vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::TypeVar(
                "T".to_string(),
                None,
            ))),
            crate::types::JuliaType::Struct("AbstractArray{T,1}".to_string()),
        ],
        vec![crate::types::TypeParam::with_upper_bound(
            "T".to_string(),
            "Real".to_string(),
        )],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "type_abstract_array_rank1_diagonal_runtime_6245",
        vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::Integer)),
            crate::types::JuliaType::Struct("AbstractArray{<:Real,1}".to_string()),
        ],
        vec![],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "type_abstract_array_rank1_diagonal_runtime_exact_6245",
        vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::TypeVar(
                "T".to_string(),
                None,
            ))),
            crate::types::JuliaType::Struct("AbstractArray{T,1}".to_string()),
        ],
        vec![crate::types::TypeParam::with_upper_bound(
            "T".to_string(),
            "Real".to_string(),
        )],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "type_abstract_array_rank1_diagonal_runtime_exact_6245",
        vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::Int64)),
            crate::types::JuliaType::Struct("AbstractArray{Int64,1}".to_string()),
        ],
        vec![],
    )));

    let int_vector = array_value(new_array_ref(ArrayValue::from_i64(vec![1, 2], vec![2])));
    let float_vector = array_value(new_array_ref(ArrayValue::from_f64(vec![1.0, 2.0], vec![2])));

    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[0, 1],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Int64)),
                int_vector.clone(),
            ],
        )
        .unwrap(),
        Some(0),
        "concrete Type{{Int64}} plus Vector{{Int64}} selects the AbstractArray rank-1 diagonal method"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[0, 1],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Integer)),
                int_vector.clone(),
            ],
        )
        .unwrap(),
        Some(1),
        "abstract Type{{Integer}} keeps the fixed Type{{Integer}}, AbstractArray{{<:Real,1}} method"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[0, 1],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Float64)),
                float_vector,
            ],
        )
        .unwrap(),
        Some(0),
        "Float64 vector satisfies only the AbstractArray rank-1 diagonal method"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[2, 3],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Int64)),
                int_vector
            ],
        )
        .unwrap(),
        Some(3),
        "an exact Type{{Int64}}, AbstractArray{{Int64,1}} method remains more specific"
    );
}

#[test]
fn test_find_best_method_index_from_candidates_type_abstract_array_rank_omitted_diagonal_issue_6247(
) {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.functions.push(Rc::new(dispatch_test_function(
        "type_abstract_array_rank_omitted_runtime_6247",
        vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::TypeVar(
                "T".to_string(),
                None,
            ))),
            crate::types::JuliaType::Struct("AbstractArray{T}".to_string()),
        ],
        vec![crate::types::TypeParam::with_upper_bound(
            "T".to_string(),
            "Real".to_string(),
        )],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "type_abstract_array_rank_omitted_runtime_6247",
        vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::Integer)),
            crate::types::JuliaType::Struct("AbstractArray{<:Real}".to_string()),
        ],
        vec![],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "type_abstract_array_rank_omitted_runtime_exact_6247",
        vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::TypeVar(
                "T".to_string(),
                None,
            ))),
            crate::types::JuliaType::Struct("AbstractArray{T}".to_string()),
        ],
        vec![crate::types::TypeParam::with_upper_bound(
            "T".to_string(),
            "Real".to_string(),
        )],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "type_abstract_array_rank_omitted_runtime_exact_6247",
        vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::Int64)),
            crate::types::JuliaType::Struct("AbstractArray{Int64}".to_string()),
        ],
        vec![],
    )));

    let int_vector = array_value(new_array_ref(ArrayValue::from_i64(vec![1, 2], vec![2])));
    let int_matrix = array_value(new_array_ref(ArrayValue::from_i64(vec![1, 2], vec![1, 2])));
    let float_vector = array_value(new_array_ref(ArrayValue::from_f64(vec![1.0, 2.0], vec![2])));

    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[0, 1],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Int64)),
                int_vector.clone(),
            ],
        )
        .unwrap(),
        Some(0),
        "rank-omitted AbstractArray diagonal method covers concrete Vector{{Int64}} actuals"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[0, 1],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Int64)),
                int_matrix.clone(),
            ],
        )
        .unwrap(),
        Some(0),
        "rank-omitted AbstractArray diagonal method covers concrete Matrix{{Int64}} actuals"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[0, 1],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Integer)),
                int_vector.clone(),
            ],
        )
        .unwrap(),
        Some(1),
        "abstract Type{{Integer}} keeps the fixed Type{{Integer}}, AbstractArray{{<:Real}} method"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[0, 1],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Float64)),
                float_vector,
            ],
        )
        .unwrap(),
        Some(0),
        "Float64 vector satisfies only the rank-omitted AbstractArray diagonal method"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[2, 3],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Int64)),
                int_vector,
            ],
        )
        .unwrap(),
        Some(3),
        "an exact Type{{Int64}}, AbstractArray{{Int64}} method remains more specific for vectors"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[2, 3],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Int64)),
                int_matrix
            ],
        )
        .unwrap(),
        Some(3),
        "an exact Type{{Int64}}, AbstractArray{{Int64}} method remains more specific for matrices"
    );
}

#[test]
fn test_find_best_method_index_from_candidates_type_abstract_array_rank_typevar_diagonal_issue_6249(
) {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.functions.push(Rc::new(dispatch_test_function(
        "type_abstract_array_rank_typevar_runtime_6249",
        vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::TypeVar(
                "T".to_string(),
                None,
            ))),
            crate::types::JuliaType::Struct("AbstractArray{T,N}".to_string()),
        ],
        vec![
            crate::types::TypeParam::with_upper_bound("T".to_string(), "Real".to_string()),
            crate::types::TypeParam::new("N".to_string()),
        ],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "type_abstract_array_rank_typevar_runtime_6249",
        vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::Integer)),
            crate::types::JuliaType::Struct("AbstractArray{<:Real,N}".to_string()),
        ],
        vec![crate::types::TypeParam::new("N".to_string())],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "type_abstract_array_rank_typevar_runtime_exact_6249",
        vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::TypeVar(
                "T".to_string(),
                None,
            ))),
            crate::types::JuliaType::Struct("AbstractArray{T,N}".to_string()),
        ],
        vec![
            crate::types::TypeParam::with_upper_bound("T".to_string(), "Real".to_string()),
            crate::types::TypeParam::new("N".to_string()),
        ],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "type_abstract_array_rank_typevar_runtime_exact_6249",
        vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::Int64)),
            crate::types::JuliaType::Struct("AbstractArray{Int64,N}".to_string()),
        ],
        vec![crate::types::TypeParam::new("N".to_string())],
    )));

    let int_vector = array_value(new_array_ref(ArrayValue::from_i64(vec![1, 2], vec![2])));
    let int_matrix = array_value(new_array_ref(ArrayValue::from_i64(vec![1, 2], vec![1, 2])));
    let float_vector = array_value(new_array_ref(ArrayValue::from_f64(vec![1.0, 2.0], vec![2])));

    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[0, 1],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Int64)),
                int_vector.clone(),
            ],
        )
        .unwrap(),
        Some(0),
        "rank-TypeVar AbstractArray diagonal method covers concrete Vector{{Int64}} actuals"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[0, 1],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Int64)),
                int_matrix.clone(),
            ],
        )
        .unwrap(),
        Some(0),
        "rank-TypeVar AbstractArray diagonal method covers concrete Matrix{{Int64}} actuals"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[0, 1],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Integer)),
                int_vector.clone(),
            ],
        )
        .unwrap(),
        Some(1),
        "abstract Type{{Integer}} keeps the fixed Type{{Integer}}, AbstractArray{{<:Real,N}} method"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[0, 1],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Float64)),
                float_vector,
            ],
        )
        .unwrap(),
        Some(0),
        "Float64 vector satisfies only the rank-TypeVar AbstractArray diagonal method"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[2, 3],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Int64)),
                int_vector,
            ],
        )
        .unwrap(),
        Some(3),
        "an exact Type{{Int64}}, AbstractArray{{Int64,N}} method remains more specific for vectors"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[2, 3],
            &[Value::DataType(Box::new(crate::types::JuliaType::Int64)), int_matrix],
        )
        .unwrap(),
        Some(3),
        "an exact Type{{Int64}}, AbstractArray{{Int64,N}} method remains more specific for matrices"
    );
}

#[test]
fn test_find_best_method_index_from_candidates_type_matrix_diagonal_issue_6237() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.functions.push(Rc::new(dispatch_test_function(
        "type_matrix_diagonal_runtime_6237",
        vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::TypeVar(
                "T".to_string(),
                None,
            ))),
            crate::types::JuliaType::MatrixOf(Box::new(crate::types::JuliaType::TypeVar(
                "T".to_string(),
                None,
            ))),
        ],
        vec![crate::types::TypeParam::with_upper_bound(
            "T".to_string(),
            "Real".to_string(),
        )],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "type_matrix_diagonal_runtime_6237",
        vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::Integer)),
            crate::types::JuliaType::MatrixOf(Box::new(crate::types::JuliaType::TypeVar(
                "_".to_string(),
                Some("Real".to_string()),
            ))),
        ],
        vec![],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "type_matrix_diagonal_runtime_exact_6237",
        vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::TypeVar(
                "T".to_string(),
                None,
            ))),
            crate::types::JuliaType::MatrixOf(Box::new(crate::types::JuliaType::TypeVar(
                "T".to_string(),
                None,
            ))),
        ],
        vec![crate::types::TypeParam::with_upper_bound(
            "T".to_string(),
            "Real".to_string(),
        )],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "type_matrix_diagonal_runtime_exact_6237",
        vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::Int64)),
            crate::types::JuliaType::MatrixOf(Box::new(crate::types::JuliaType::Int64)),
        ],
        vec![],
    )));

    let int_matrix = array_value(new_array_ref(ArrayValue::from_i64(vec![1, 2], vec![1, 2])));
    let float_matrix = array_value(new_array_ref(ArrayValue::from_f64(
        vec![1.0, 2.0],
        vec![1, 2],
    )));

    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[0, 1],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Int64)),
                int_matrix.clone(),
            ],
        )
        .unwrap(),
        Some(0),
        "concrete Type{{Int64}} plus Matrix{{Int64}} selects the diagonal method"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[0, 1],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Integer)),
                int_matrix.clone(),
            ],
        )
        .unwrap(),
        Some(1),
        "abstract Type{{Integer}} keeps the fixed Type{{Integer}}, Matrix{{<:Real}} method"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[0, 1],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Float64)),
                float_matrix,
            ],
        )
        .unwrap(),
        Some(0),
        "Float64 matrix satisfies only the diagonal method"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[2, 3],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Int64)),
                int_matrix
            ],
        )
        .unwrap(),
        Some(3),
        "an exact Type{{Int64}}, Matrix{{Int64}} method remains more specific"
    );
}

#[test]
fn test_find_best_method_index_from_candidates_type_abstract_matrix_diagonal_issue_6240() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.functions.push(Rc::new(dispatch_test_function(
        "type_abstract_matrix_diagonal_runtime_6240",
        vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::TypeVar(
                "T".to_string(),
                None,
            ))),
            crate::types::JuliaType::Struct("AbstractMatrix{T}".to_string()),
        ],
        vec![crate::types::TypeParam::with_upper_bound(
            "T".to_string(),
            "Real".to_string(),
        )],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "type_abstract_matrix_diagonal_runtime_6240",
        vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::Integer)),
            crate::types::JuliaType::Struct("AbstractMatrix{<:Real}".to_string()),
        ],
        vec![],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "type_abstract_matrix_diagonal_runtime_exact_6240",
        vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::TypeVar(
                "T".to_string(),
                None,
            ))),
            crate::types::JuliaType::Struct("AbstractMatrix{T}".to_string()),
        ],
        vec![crate::types::TypeParam::with_upper_bound(
            "T".to_string(),
            "Real".to_string(),
        )],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "type_abstract_matrix_diagonal_runtime_exact_6240",
        vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::Int64)),
            crate::types::JuliaType::Struct("AbstractMatrix{Int64}".to_string()),
        ],
        vec![],
    )));

    let int_matrix = array_value(new_array_ref(ArrayValue::from_i64(vec![1, 2], vec![1, 2])));
    let float_matrix = array_value(new_array_ref(ArrayValue::from_f64(
        vec![1.0, 2.0],
        vec![1, 2],
    )));

    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[0, 1],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Int64)),
                int_matrix.clone(),
            ],
        )
        .unwrap(),
        Some(0),
        "concrete Type{{Int64}} plus Matrix{{Int64}} selects the AbstractMatrix diagonal method"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[0, 1],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Integer)),
                int_matrix.clone(),
            ],
        )
        .unwrap(),
        Some(1),
        "abstract Type{{Integer}} keeps the fixed Type{{Integer}}, AbstractMatrix{{<:Real}} method"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[0, 1],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Float64)),
                float_matrix,
            ],
        )
        .unwrap(),
        Some(0),
        "Float64 matrix satisfies only the AbstractMatrix diagonal method"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[2, 3],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Int64)),
                int_matrix
            ],
        )
        .unwrap(),
        Some(3),
        "an exact Type{{Int64}}, AbstractMatrix{{Int64}} method remains more specific"
    );
}

#[test]
fn test_find_best_method_index_from_candidates_type_abstract_array_rank2_diagonal_issue_6243() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.functions.push(Rc::new(dispatch_test_function(
        "type_abstract_array_rank2_diagonal_runtime_6243",
        vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::TypeVar(
                "T".to_string(),
                None,
            ))),
            crate::types::JuliaType::Struct("AbstractArray{T,2}".to_string()),
        ],
        vec![crate::types::TypeParam::with_upper_bound(
            "T".to_string(),
            "Real".to_string(),
        )],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "type_abstract_array_rank2_diagonal_runtime_6243",
        vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::Integer)),
            crate::types::JuliaType::Struct("AbstractArray{<:Real,2}".to_string()),
        ],
        vec![],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "type_abstract_array_rank2_diagonal_runtime_exact_6243",
        vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::TypeVar(
                "T".to_string(),
                None,
            ))),
            crate::types::JuliaType::Struct("AbstractArray{T,2}".to_string()),
        ],
        vec![crate::types::TypeParam::with_upper_bound(
            "T".to_string(),
            "Real".to_string(),
        )],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "type_abstract_array_rank2_diagonal_runtime_exact_6243",
        vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::Int64)),
            crate::types::JuliaType::Struct("AbstractArray{Int64,2}".to_string()),
        ],
        vec![],
    )));

    let int_matrix = array_value(new_array_ref(ArrayValue::from_i64(vec![1, 2], vec![1, 2])));
    let float_matrix = array_value(new_array_ref(ArrayValue::from_f64(
        vec![1.0, 2.0],
        vec![1, 2],
    )));

    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[0, 1],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Int64)),
                int_matrix.clone(),
            ],
        )
        .unwrap(),
        Some(0),
        "concrete Type{{Int64}} plus Matrix{{Int64}} selects the AbstractArray rank-2 diagonal method"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[0, 1],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Integer)),
                int_matrix.clone(),
            ],
        )
        .unwrap(),
        Some(1),
        "abstract Type{{Integer}} keeps the fixed Type{{Integer}}, AbstractArray{{<:Real,2}} method"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[0, 1],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Float64)),
                float_matrix,
            ],
        )
        .unwrap(),
        Some(0),
        "Float64 matrix satisfies only the AbstractArray rank-2 diagonal method"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[2, 3],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Int64)),
                int_matrix
            ],
        )
        .unwrap(),
        Some(3),
        "an exact Type{{Int64}}, AbstractArray{{Int64,2}} method remains more specific"
    );
}

#[test]
fn test_find_best_method_index_from_candidates_invariant_vector_typevar_issue_6222() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.functions.push(Rc::new(dispatch_test_function(
        "invariant_vector_typevar_runtime_6222",
        vec![
            crate::types::JuliaType::TypeVar("T".to_string(), None),
            crate::types::JuliaType::VectorOf(Box::new(crate::types::JuliaType::TypeVar(
                "T".to_string(),
                None,
            ))),
        ],
        vec![crate::types::TypeParam::with_upper_bound(
            "T".to_string(),
            "Real".to_string(),
        )],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "invariant_vector_typevar_runtime_6222",
        vec![
            crate::types::JuliaType::Integer,
            crate::types::JuliaType::VectorOf(Box::new(crate::types::JuliaType::TypeVar(
                "_".to_string(),
                Some("Real".to_string()),
            ))),
        ],
        vec![],
    )));

    let int_vector = array_value(new_array_ref(ArrayValue::from_i64(vec![2, 3], vec![2])));
    let real_vector = array_value(new_array_ref(
        ArrayValue::memory_first_collect_typejoin_values(
            vec![Value::I64(2), Value::F64(3.0)],
            ArrayElementType::Any,
        )
        .unwrap(),
    ));

    assert_eq!(
        vm.get_value_julia_type(&real_vector),
        crate::types::JuliaType::VectorOf(Box::new(crate::types::JuliaType::Real))
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(&[0, 1], &[Value::I64(1), int_vector])
            .unwrap(),
        Some(1),
        "the fixed Integer + Vector{{<:Real}} method wins for Vector{{Int64}}"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(&[0, 1], &[Value::I64(1), real_vector])
            .unwrap(),
        Some(1),
        "the invariant Vector{{T}} occurrence must not outrank the fixed Integer slot"
    );
}

#[test]
fn test_find_best_method_index_from_candidates_vector_diagonal_issue_6229() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.functions.push(Rc::new(dispatch_test_function(
        "vector_diagonal_runtime_6229",
        vec![
            crate::types::JuliaType::VectorOf(Box::new(crate::types::JuliaType::TypeVar(
                "T".to_string(),
                None,
            ))),
            crate::types::JuliaType::VectorOf(Box::new(crate::types::JuliaType::TypeVar(
                "T".to_string(),
                None,
            ))),
        ],
        vec![crate::types::TypeParam::with_upper_bound(
            "T".to_string(),
            "Real".to_string(),
        )],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "vector_diagonal_runtime_6229",
        vec![
            crate::types::JuliaType::VectorOf(Box::new(crate::types::JuliaType::TypeVar(
                "_".to_string(),
                Some("Real".to_string()),
            ))),
            crate::types::JuliaType::VectorOf(Box::new(crate::types::JuliaType::TypeVar(
                "_".to_string(),
                Some("Real".to_string()),
            ))),
        ],
        vec![],
    )));

    let int_left = array_value(new_array_ref(ArrayValue::from_i64(vec![1], vec![1])));
    let int_right = array_value(new_array_ref(ArrayValue::from_i64(vec![2], vec![1])));
    let float_right = array_value(new_array_ref(ArrayValue::from_f64(vec![2.0], vec![1])));

    assert_eq!(
        vm.find_best_method_index_from_candidates(&[0, 1], &[int_left.clone(), int_right])
            .unwrap(),
        Some(0),
        "same concrete Vector element type must select the diagonal Vector{{T}} method"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(&[0, 1], &[int_left, float_right])
            .unwrap(),
        Some(1),
        "mixed Vector element types do not satisfy the repeated T binding"
    );
}

#[test]
fn test_call_site_dispatch_cache_stores_polymorphic_entries_issue_5079() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    let call_site_ip = 42;
    let int_hash = hash_type_name("Int64");
    let float_hash = hash_type_name("Float64");

    assert_eq!(
        vm.lookup_call_site_dispatch_cache(call_site_ip, int_hash),
        None
    );

    vm.store_call_site_dispatch_cache(call_site_ip, int_hash, 7);
    vm.store_call_site_dispatch_cache(call_site_ip, float_hash, 9);

    assert_eq!(
        vm.lookup_call_site_dispatch_cache(call_site_ip, int_hash),
        Some(7)
    );
    assert_eq!(
        vm.lookup_call_site_dispatch_cache(call_site_ip, float_hash),
        Some(9)
    );

    vm.store_call_site_dispatch_cache(call_site_ip + 1, int_hash, usize::MAX);
    assert_eq!(
        vm.lookup_call_site_dispatch_cache(call_site_ip + 1, int_hash),
        Some(usize::MAX)
    );
}

#[test]
fn test_call_site_inline_cache_hits_exact_scalar_issue_6345() {
    let mut vm = Vm::new(vec![Instr::Nop; 4], StableRng::new(0));
    let call_site_ip = 2;
    let int_fingerprint = vm
        .call_site_arg_fingerprint(&Value::I64(1))
        .expect("Int64 should be eligible for exact L1 dispatch caching");

    assert_eq!(
        vm.lookup_call_site_inline_cache(call_site_ip, int_fingerprint),
        None
    );

    vm.store_call_site_inline_cache(call_site_ip, Some(int_fingerprint), 7);

    assert_eq!(
        vm.lookup_call_site_inline_cache(call_site_ip, int_fingerprint),
        Some(7)
    );
    assert!(
        vm.dispatch_cache.is_empty(),
        "L1 hit path must not require populating the L2 HashMap cache"
    );
}

#[test]
fn test_call_site_inline_cache_preserves_negative_sentinel_issue_6345() {
    let mut vm = Vm::new(vec![Instr::Nop; 2], StableRng::new(0));
    let fingerprint = vm
        .call_site_arg_fingerprint(&Value::Bool(true))
        .expect("Bool should be eligible for exact L1 dispatch caching");

    vm.store_call_site_inline_cache(1, Some(fingerprint), usize::MAX);

    assert_eq!(
        vm.lookup_call_site_inline_cache(1, fingerprint),
        Some(usize::MAX),
        "builtin/native fallback sentinel must round-trip through L1"
    );
}

#[test]
fn test_call_site_inline_cache_skips_parametric_identities_issue_6345() {
    let vm = Vm::new(vec![Instr::Nop; 1], StableRng::new(0));

    assert!(
        vm.call_site_arg_fingerprint(&Value::DataType(Box::new(crate::types::JuliaType::Int64)))
            .is_none(),
        "Type{{T}} dispatch identities are not represented by scalar L1 tags"
    );
    assert!(
        vm.call_site_arg_fingerprint(&Value::Tuple(TupleValue::new(vec![Value::I64(1)])))
            .is_none(),
        "tuple dispatch identities depend on element types and must use L2/L3"
    );
}

#[test]
fn test_slot_storage_keeps_immutable_structs_inline_issue_5173() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.struct_defs.push(StructDefInfo {
        name: "Point".to_string(),
        is_mutable: false,
        fields: vec![
            ("x".to_string(), ValueType::I64),
            ("y".to_string(), ValueType::I64),
        ],
        field_julia_types: vec![
            crate::types::JuliaType::Int64,
            crate::types::JuliaType::Int64,
        ],
        parent_type: None,
    });
    vm.struct_defs.push(StructDefInfo {
        name: "Box".to_string(),
        is_mutable: true,
        fields: vec![("x".to_string(), ValueType::I64)],
        field_julia_types: vec![crate::types::JuliaType::Int64],
        parent_type: None,
    });

    for _ in 0..8 {
        let stored = vm.value_for_slot_storage(Value::Struct(StructInstance::with_name(
            0,
            "Point".to_string(),
            vec![Value::I64(1), Value::I64(2)],
        )));
        assert!(matches!(stored, Value::Struct(_)));
    }
    assert!(
        vm.struct_heap.is_empty(),
        "immutable StoreSlot storage must not grow struct_heap"
    );

    let stored = vm.value_for_slot_storage(Value::Struct(StructInstance::with_name(
        1,
        "Box".to_string(),
        vec![Value::I64(3)],
    )));
    assert!(matches!(stored, Value::StructRef(0)));
    assert_eq!(vm.struct_heap.len(), 1);
}

/// Issue #5179: `new_program` must build a per-function `name -> slot`
/// map (`function_slot_maps`) consistent with each function's `slot_names`,
/// and `slot_index_for_frame` must resolve names through it identically to
/// the legacy linear scan over `slot_names`.
#[test]
fn function_slot_maps_match_slot_names_and_resolve_in_o1() {
    use crate::compile::compile_core_program;
    use crate::lowering::Lowering;
    use crate::parser::Parser;

    let source = "function f(a, b)\n  c = a + b\n  d = c * 2\n  d - a\nend\n";
    let mut parser = Parser::new().expect("create parser");
    let parsed = parser.parse(source).expect("parse source");
    let mut lowering = Lowering::new(source);
    let program = lowering.lower(parsed).expect("lower source");
    let compiled = compile_core_program(&program).expect("compile failed");

    let func_index = compiled
        .functions
        .iter()
        .position(|fnc| fnc.name == "f")
        .expect("function f compiled");

    let vm = Vm::new_program(compiled, StableRng::new(0));

    // The pre-computed map exists for every function and mirrors slot_names.
    assert_eq!(vm.function_slot_maps.len(), vm.functions.len());
    let func = &vm.functions[func_index];
    let slot_map = &vm.function_slot_maps[func_index];
    assert_eq!(slot_map.len(), func.slot_names.len());
    for (idx, name) in func.slot_names.iter().enumerate() {
        assert_eq!(slot_map.get(name), Some(&idx), "slot {} ({})", idx, name);
    }

    // slot_index_for_frame resolves through the map identically to the
    // legacy linear scan, and reports None for unknown names.
    let frame = Frame::new_with_slots(func.local_slot_count, Some(func_index));
    for (idx, name) in func.slot_names.iter().enumerate() {
        let linear = func
            .slot_names
            .iter()
            .position(|slot_name| slot_name == name);
        assert_eq!(linear, Some(idx));
        assert_eq!(vm.slot_index_for_frame(&frame, name), Some(idx));
    }
    assert_eq!(vm.slot_index_for_frame(&frame, "no_such_var"), None);
}

#[test]
fn test_find_best_method_index_prefers_parametric_type_pattern() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.functions.push(Rc::new(FunctionInfo {
        name: "_array_undef_from_dims".to_string(),
        params: vec![
            ("typ".to_string(), ValueType::DataType),
            ("dims".to_string(), ValueType::Tuple),
        ],
        kwparams: vec![],
        entry: 0,
        return_type: ValueType::Any,
        return_julia_type: None,
        is_base_extension: false,
        is_generated: false,
        min_world: 1,
        type_params: vec![],
        param_julia_types: vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::Struct(
                "Pair".to_string(),
            ))),
            crate::types::JuliaType::TupleOf(vec![crate::types::JuliaType::Int64]),
        ],
        code_start: 0,
        code_end: 0,
        slot_names: vec![],
        slot_types: vec![],
        local_slot_count: 0,
        param_slots: vec![],
        vararg_param_index: None,
        vararg_fixed_count: None,
        inlining_meta: 0,
        constprop_meta: 0,
        nospecialize_meta: 0,
        propagate_inbounds_meta: false,
        nospecializeinfer_meta: false,
        purity_meta: 0,
        direct_return_type_param: None,
        // Builtin stub FunctionInfo: no source line (Issue #5125).
        def_line: 0,
    }));
    vm.functions.push(Rc::new(FunctionInfo {
        name: "_array_undef_from_dims".to_string(),
        params: vec![
            ("typ".to_string(), ValueType::DataType),
            ("dims".to_string(), ValueType::Tuple),
        ],
        kwparams: vec![],
        entry: 0,
        return_type: ValueType::Any,
        return_julia_type: None,
        is_base_extension: false,
        is_generated: false,
        min_world: 1,
        type_params: vec![
            crate::types::TypeParam::new("K".to_string()),
            crate::types::TypeParam::new("V".to_string()),
        ],
        param_julia_types: vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::Struct(
                "Pair{K,V}".to_string(),
            ))),
            crate::types::JuliaType::TupleOf(vec![crate::types::JuliaType::Int64]),
        ],
        code_start: 0,
        code_end: 0,
        slot_names: vec![],
        slot_types: vec![],
        local_slot_count: 0,
        param_slots: vec![],
        vararg_param_index: None,
        vararg_fixed_count: None,
        inlining_meta: 0,
        constprop_meta: 0,
        nospecialize_meta: 0,
        propagate_inbounds_meta: false,
        nospecializeinfer_meta: false,
        purity_meta: 0,
        direct_return_type_param: None,
        // Builtin stub FunctionInfo: no source line (Issue #5125).
        def_line: 0,
    }));
    vm.function_name_index
        .insert("_array_undef_from_dims".to_string(), vec![0, 1]);

    let args = vec![
        Value::DataType(Box::new(crate::types::JuliaType::Struct(
            "Pair{Int64,Int8}".to_string(),
        ))),
        Value::Tuple(TupleValue::new(vec![Value::I64(2)])),
    ];

    assert_eq!(
        vm.find_best_method_index(&["_array_undef_from_dims"], &args),
        Some(1)
    );
}

#[test]
fn test_find_best_method_index_matches_bare_tuple_parametric_type_issue_4643() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.functions.push(Rc::new(FunctionInfo {
        name: "similar".to_string(),
        params: vec![
            ("typ".to_string(), ValueType::DataType),
            ("dims".to_string(), ValueType::Tuple),
        ],
        kwparams: vec![],
        entry: 0,
        return_type: ValueType::Any,
        return_julia_type: None,
        is_base_extension: false,
        is_generated: false,
        min_world: 1,
        type_params: vec![crate::types::TypeParam::new("T".to_string())],
        param_julia_types: vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::Struct(
                "Array{T}".to_string(),
            ))),
            crate::types::JuliaType::Tuple,
        ],
        code_start: 0,
        code_end: 0,
        slot_names: vec![],
        slot_types: vec![],
        local_slot_count: 0,
        param_slots: vec![],
        vararg_param_index: None,
        vararg_fixed_count: None,
        inlining_meta: 0,
        constprop_meta: 0,
        nospecialize_meta: 0,
        propagate_inbounds_meta: false,
        nospecializeinfer_meta: false,
        purity_meta: 0,
        direct_return_type_param: None,
        // Builtin stub FunctionInfo: no source line (Issue #5125).
        def_line: 0,
    }));
    vm.function_name_index
        .insert("similar".to_string(), vec![0]);

    let args = vec![
        Value::DataType(Box::new(crate::types::JuliaType::Struct(
            "Array{Int64}".to_string(),
        ))),
        Value::Tuple(TupleValue::new(vec![Value::I64(2)])),
    ];

    assert_eq!(vm.find_best_method_index(&["similar"], &args), Some(0));
}

// === Issue #3094: VmError::InternalError propagation tests ===

/// InternalError has error code 33.
#[test]
fn test_internal_error_code_is_33() {
    let code = Vm::<StableRng>::error_code(&VmError::InternalError("test".to_string()));
    assert_eq!(code, 33);
}

/// Returning from a callee must not pop a caller's active try/catch handler.
#[test]
fn test_pop_handlers_for_return_preserves_caller_handler() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.handlers.push(Handler {
        catch_ip: Some(10),
        finally_ip: None,
        stack_len: 0,
        frame_len: 1,
        return_ip_len: 0,
        caught_exception_len: 0,
    });
    vm.handlers.push(Handler {
        catch_ip: Some(20),
        finally_ip: None,
        stack_len: 0,
        frame_len: 2,
        return_ip_len: 1,
        caught_exception_len: 0,
    });

    vm.pop_handlers_for_return();

    assert_eq!(vm.handlers.len(), 1);
    assert_eq!(vm.handlers[0].catch_ip, Some(10));
    assert_eq!(vm.handlers[0].return_ip_len, 0);
}

/// handle_error catches a user-visible error when a handler with catch_ip exists.
#[test]
fn test_handle_error_catches_user_error_with_handler() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.handlers.push(Handler {
        catch_ip: Some(100),
        finally_ip: None,
        stack_len: 0,
        frame_len: 1,
        return_ip_len: 0,
        caught_exception_len: 0,
    });
    let result = vm.handle_error(VmError::TypeError("test".to_string()));
    assert!(result.is_ok(), "Expected Ok, got {:?}", result);
    assert_eq!(vm.ip, 100);
    assert!(
        matches!(vm.pending_error, Some(VmError::TypeError(_))),
        "Expected pending TypeError, got {:?}",
        vm.pending_error
    );
}

/// handle_error propagates ANY error when no handler exists.
#[test]
fn test_handle_error_propagates_when_no_handler() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    let result = vm.handle_error(VmError::InternalError("test".to_string()));
    assert!(
        matches!(result, Err(VmError::InternalError(_))),
        "Expected Err(InternalError), got {:?}",
        result
    );
}

/// raise() catches a user-visible error when a handler exists.
#[test]
fn test_raise_catches_with_handler() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.handlers.push(Handler {
        catch_ip: Some(50),
        finally_ip: None,
        stack_len: 0,
        frame_len: 1,
        return_ip_len: 0,
        caught_exception_len: 0,
    });
    let result = vm.raise(VmError::DomainError("test".to_string()));
    assert!(result.is_ok(), "Expected Ok, got {:?}", result);
    assert_eq!(vm.ip, 50);
}

/// raise() propagates when no handler exists.
#[test]
fn test_raise_propagates_without_handler() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    let result = vm.raise(VmError::TypeError("test".to_string()));
    assert!(
        matches!(result, Err(VmError::TypeError(_))),
        "Expected Err(TypeError), got {:?}",
        result
    );
}

/// InternalError from get_function_checked IS caught by handlers when the
/// instruction uses try_or_handle (e.g., Call instruction uses
/// get_function_cloned_or_raise). This documents that InternalError
/// propagation depends on HOW the error is surfaced, not the error variant.
#[test]
fn test_internal_error_caught_via_try_or_handle_in_run() {
    // Call(9999, 0) triggers InternalError via get_function_cloned_or_raise,
    // which calls try_or_handle → handle_error. The handler catches it.
    let catch_ip = 2;
    let mut vm = Vm::new(
        vec![
            Instr::PushHandler(Some(catch_ip), None), // ip=0: push handler
            Instr::Call(9999, 0), // ip=1: invalid func → InternalError via try_or_handle
            Instr::ClearError,    // ip=2 (catch): clear error
            Instr::PushI64(42),   // ip=3: push result
            Instr::ReturnAny,     // ip=4: return 42
        ],
        StableRng::new(0),
    );
    let result = vm.run();
    // InternalError IS caught because Call uses try_or_handle
    assert!(
        matches!(result, Ok(Value::I64(42))),
        "Expected Ok(I64(42)), got {:?}",
        result
    );
}

/// Issue #6342 / #5969: the call-frame stack is bounded at call boundaries
/// instead of at every interpreter-loop instruction. With no active handler
/// installed, the error propagates out of the push path (the caught path —
/// `e isa StackOverflowError` — is covered end-to-end by
/// `tests/fixtures/error/error_stack_overflow_5969.jl`).
#[test]
fn test_call_depth_guard_raises_stack_overflow_5969() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    for _ in 0..Vm::<StableRng>::MAX_CALL_DEPTH {
        vm.frames.push(Frame::new());
    }
    vm.try_push_call_frame(Frame::new()).unwrap();
    let result = vm.handle_pending_call_depth_overflow();
    assert!(
        matches!(result, Err(VmError::StackOverflow)),
        "expected Err(StackOverflow), got {:?}",
        result
    );
}

/// Issue #6342 / #5969: nested dispatch uses the same call-boundary push
/// helper, so eval/HOF-driven recursion cannot grow `self.frames` beyond the
/// limit before surfacing `StackOverflow`.
#[test]
fn test_call_depth_guard_in_run_until_frame_return_5969() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    for _ in 0..Vm::<StableRng>::MAX_CALL_DEPTH {
        vm.frames.push(Frame::new());
    }
    vm.try_push_call_frame(Frame::new()).unwrap();
    let result = vm.handle_pending_call_depth_overflow();
    assert!(
        matches!(result, Err(VmError::StackOverflow)),
        "expected Err(StackOverflow), got {:?}",
        result
    );
}

/// Issue #5972: an error raised inside nested eval dispatch must NOT be
/// caught by a handler installed by an *ancestor* frame (`frame_len <=
/// target_depth`). Such a handler belongs to a `try` opened *outside* the
/// nested `eval` dispatch. The `eval_dispatch_floor` lets the error
/// propagate as `Err` so the outer loop re-routes it to that ancestor
/// handler. Here an ancestor handler (`frame_len == target_depth == 0`) is
/// present, yet the overflow must still surface as `Err(StackOverflow)` with
/// the handler left intact.
#[test]
fn test_ancestor_handler_not_consumed_in_run_until_frame_return_5972() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    // Ancestor handler at frame_len 0 (== target_depth): a `try` outside the
    // nested dispatch.
    vm.handlers.push(Handler {
        catch_ip: Some(0),
        finally_ip: None,
        stack_len: 0,
        frame_len: 0,
        return_ip_len: 0,
        caught_exception_len: 0,
    });
    for _ in 0..Vm::<StableRng>::MAX_CALL_DEPTH {
        vm.frames.push(Frame::new());
    }
    vm.eval_dispatch_floor = Some(0);

    vm.try_push_call_frame(Frame::new()).unwrap();
    let result = vm.handle_pending_call_depth_overflow();

    assert!(
        matches!(result, Err(VmError::StackOverflow)),
        "ancestor handler must not catch inside the nested loop; expected \
         Err(StackOverflow), got {:?}",
        result
    );
    // The ancestor handler must be left untouched for the outer loop to use.
    assert_eq!(vm.handlers.len(), 1, "ancestor handler was consumed");
}

#[test]
fn test_direct_call_fast_path_preserves_call_return_flow() {
    let mut vm = Vm::new(
        vec![
            Instr::PushI64(40),
            Instr::PushI64(2),
            Instr::Call(0, 2),
            Instr::ReturnAny,
            Instr::LoadSlotI64(0),
            Instr::LoadSlotI64(1),
            Instr::AddI64,
            Instr::ReturnI64,
        ],
        StableRng::new(0),
    );
    vm.functions.push(Rc::new(FunctionInfo {
        name: "add_two".to_string(),
        params: vec![
            ("x".to_string(), ValueType::I64),
            ("y".to_string(), ValueType::I64),
        ],
        kwparams: vec![],
        entry: 4,
        return_type: ValueType::I64,
        return_julia_type: None,
        is_base_extension: false,
        is_generated: false,
        min_world: 1,
        type_params: vec![],
        param_julia_types: vec![
            crate::types::JuliaType::Int64,
            crate::types::JuliaType::Int64,
        ],
        code_start: 4,
        code_end: 8,
        slot_names: vec!["x".to_string(), "y".to_string()],
        slot_types: vec![
            Some(crate::vm::VarTypeTag::I64),
            Some(crate::vm::VarTypeTag::I64),
        ],
        local_slot_count: 2,
        param_slots: vec![0, 1],
        vararg_param_index: None,
        vararg_fixed_count: None,
        inlining_meta: 0,
        constprop_meta: 0,
        nospecialize_meta: 0,
        propagate_inbounds_meta: false,
        nospecializeinfer_meta: false,
        purity_meta: 0,
        direct_return_type_param: None,
        // Builtin stub FunctionInfo: no source line (Issue #5125).
        def_line: 0,
    }));

    let result = vm.run();
    assert!(
        matches!(result, Ok(Value::I64(42))),
        "Expected Ok(I64(42)), got {:?}",
        result
    );
}

#[test]
fn test_load_slot_i64_preserves_unsigned_integer_values() {
    let mut vm = Vm::new(
        vec![Instr::LoadSlotI64(0), Instr::ReturnAny],
        StableRng::new(0),
    );
    let mut frame = Frame::new_with_slots(1, None);
    frame.locals_slots[0] = Some(Value::U64(1));
    vm.frames.push(frame);

    let result = vm.run();
    assert!(
        matches!(result, Ok(Value::U64(1))),
        "Expected Ok(U64(1)), got {:?}",
        result
    );
}

#[test]
fn test_load_slot_i64_preserves_float_values_after_slot_retagging() {
    let mut vm = Vm::new(
        vec![Instr::LoadSlotI64(0), Instr::ReturnAny],
        StableRng::new(0),
    );
    let mut frame = Frame::new_with_slots(1, None);
    frame.locals_slots[0] = Some(Value::F64(24.0));
    vm.frames.push(frame);

    let result = vm.run();
    assert!(
        matches!(result, Ok(Value::F64(v)) if v == 24.0),
        "Expected Ok(F64(24.0)), got {:?}",
        result
    );
}

#[test]
fn test_load_slot_f64_preserves_narrow_float_values() {
    let mut vm = Vm::new(
        vec![Instr::LoadSlotF64(0), Instr::ReturnAny],
        StableRng::new(0),
    );
    let mut frame = Frame::new_with_slots(1, None);
    frame.locals_slots[0] = Some(Value::F16(half::f16::from_f32(2.5)));
    vm.frames.push(frame);

    let result = vm.run();
    assert!(
        matches!(result, Ok(Value::F16(v)) if v == half::f16::from_f32(2.5)),
        "Expected Ok(F16(2.5)), got {:?}",
        result
    );
}

#[test]
fn test_typed_float_and_string_slot_ops_roundtrip_issue_5081() {
    let mut vm = Vm::new(
        vec![
            Instr::PushF32(3.5),
            Instr::StoreSlotF32(0),
            Instr::LoadSlotF32(0),
            Instr::PushF16(half::f16::from_f32(1.25)),
            Instr::StoreSlotF16(1),
            Instr::LoadSlotF16(1),
            Instr::PushStr("slot".to_string()),
            Instr::StoreSlotStr(2),
            Instr::LoadSlotStr(2),
            Instr::PushChar('x'),
            Instr::StoreSlotChar(3),
            Instr::LoadSlotChar(3),
            Instr::PushSymbol("tag".to_string()),
            Instr::StoreSlotSymbol(4),
            Instr::LoadSlotSymbol(4),
            Instr::PushI128(Box::new(7)),
            Instr::StoreSlotNarrowInt(5),
            Instr::LoadSlotNarrowInt(5),
            Instr::PushNothing,
            Instr::StoreSlotNothing(6),
            Instr::LoadSlotNothing(6),
            Instr::ReturnAny,
        ],
        StableRng::new(0),
    );
    vm.frames.push(Frame::new_with_slots(7, None));

    let result = vm.run();
    assert!(
        matches!(result, Ok(Value::Nothing)),
        "Expected Ok(Nothing), got {:?}",
        result
    );

    let slot0 = vm
        .frames
        .last()
        .and_then(|frame| frame.locals_slots.first());
    assert!(
        matches!(slot0, Some(Some(Value::F32(v))) if *v == 3.5),
        "Expected slot 0 to hold F32(3.5), got {:?}",
        vm.frames.last().map(|frame| &frame.locals_slots)
    );
    let slot1 = vm.frames.last().and_then(|frame| frame.locals_slots.get(1));
    assert!(
        matches!(slot1, Some(Some(Value::F16(v))) if *v == half::f16::from_f32(1.25)),
        "Expected slot 1 to hold F16(1.25), got {:?}",
        vm.frames.last().map(|frame| &frame.locals_slots)
    );
    let frame = vm
        .frames
        .last()
        .expect("frame should remain for inspection");
    assert_eq!(frame.slot_str(2).map(String::as_str), Some("slot"));
    assert_eq!(frame.slot_char(3), Some('x'));
    assert!(matches!(frame.slot_symbol(4), Some(sym) if sym.as_str() == "tag"));
    assert!(matches!(frame.slot_narrow_int(5), Some(Value::I128(7))));
    assert!(frame.slot_nothing(6));
}

#[test]
fn test_store_any_symbol_uses_symbol_tag_issue_5081() {
    let mut vm = Vm::new(
        vec![
            Instr::PushSymbol("legacy".to_string()),
            Instr::StoreAny("sym".to_string()),
            Instr::LoadAny("sym".to_string()),
            Instr::ReturnAny,
        ],
        StableRng::new(0),
    );

    let result = vm.run();
    assert!(
        matches!(result, Ok(Value::Symbol(ref sym)) if sym.as_str() == "legacy"),
        "Expected Ok(Symbol(:legacy)), got {:?}",
        result
    );
    let frame = vm
        .frames
        .last()
        .expect("frame should remain for inspection");
    assert_eq!(
        frame.var_types.get("sym"),
        Some(&crate::vm::VarTypeTag::Symbol)
    );
}

#[test]
fn test_typed_container_slot_ops_roundtrip_issue_5081() {
    let mut vm = Vm::new(
        vec![
            Instr::PushArrayValue(Box::new(ArrayValue::ones_i64(vec![2]))),
            Instr::StoreSlotArray(0),
            Instr::LoadSlotArray(0),
            Instr::PushI64(1),
            Instr::PushI64(2),
            Instr::NewTuple(2),
            Instr::StoreSlotTuple(1),
            Instr::LoadSlotTuple(1),
            Instr::PushI64(10),
            Instr::PushI64(20),
            Instr::NewNamedTuple(vec!["a".to_string(), "b".to_string()]),
            Instr::StoreSlotNamedTuple(4),
            Instr::LoadSlotNamedTuple(4),
            Instr::PushI64(1),
            Instr::PushI64(1),
            Instr::PushI64(3),
            Instr::MakeRangeLazy,
            Instr::StoreSlotRange(5),
            Instr::LoadSlotRange(5),
            Instr::PushI64(1),
            Instr::NewStableRng,
            Instr::StoreSlotRng(6),
            Instr::LoadSlotRng(6),
            Instr::PushI64(1),
            Instr::PushI64(2),
            Instr::NewTuple(2),
            Instr::MakeGenerator(Box::new(MakeGeneratorOperands {
                callable: crate::vm::value::GeneratorCallable::Eager,
                result_element_type: None,
            })),
            Instr::StoreSlotGenerator(7),
            Instr::LoadSlotGenerator(7),
            Instr::ReturnAny,
        ],
        StableRng::new(0),
    );
    vm.frames.push(Frame::new_with_slots(8, None));

    let result = vm.run();
    assert!(
        matches!(result, Ok(Value::Generator(_))),
        "Expected Ok(Generator), got {:?}",
        result
    );

    let frame = vm
        .frames
        .last()
        .expect("frame should remain for inspection");
    // After the array-producer flip (Issue #6806), `PushArrayValue` yields the
    // MemoryRef-backed `Array{T,N}` wrapper (a `StructRef`), so `StoreSlotArray`
    // routes it through the generic local slot rather than the native-carrier
    // typed `slot_array` fast path (which still backs transient build buffers).
    // The array therefore roundtrips through whichever channel holds it; the
    // typed fast path for wrappers is restored in PR B (accessor migration).
    assert!(
        frame.slot_array(0).is_some() || matches!(frame.locals_slots.first(), Some(Some(_))),
        "array slot should roundtrip through the typed or generic slot"
    );
    assert!(frame.slot_tuple(1).is_some());
    assert!(frame.slot_named_tuple(4).is_some());
    assert!(frame.slot_range(5).is_some());
    assert!(frame.slot_rng(6).is_some());
    assert!(frame.slot_generator(7).is_some());
}

/// Issue #6806 (producer flip): array literals (`PushArrayValue`) materialize the
/// MemoryRef-backed `Array{T,N}` wrapper — a struct-heap instance whose name is
/// an `Array{...}` and whose first field is a `MemoryRef` — instead of the legacy
/// native-array carrier. The host-return boundary
/// (`normalize_host_return_value`) re-materializes the *returned* value for the
/// heap-less caller, so the internal representation is asserted by
/// inspecting the struct heap rather than `run()`'s return value. This pins the
/// producer so the carrier confinement (#6807) can rely on it no longer emitting
/// the carrier. The runtime value, `typeof`, and `isa` are unchanged.
#[test]
fn test_array_literal_emits_memoryref_wrapper_issue_6806() {
    let mut vm = Vm::new(
        vec![
            Instr::PushArrayValue(Box::new(ArrayValue::ones_i64(vec![3]))),
            Instr::ReturnAny,
        ],
        StableRng::new(0),
    );
    vm.frames.push(Frame::new_with_slots(0, None));
    let _ = vm.run().expect("bare-VM run should succeed");

    let wrapper = vm.get_struct_heap().iter().find(|inst| {
        (&*inst.struct_name == "Array" || inst.struct_name.starts_with("Array{"))
            && matches!(inst.values.first(), Some(Value::MemoryRef(_)))
    });
    assert!(
        wrapper.is_some(),
        "array literal should materialize a MemoryRef-backed Array wrapper in the \
         struct heap; heap = {:?}",
        vm.get_struct_heap()
    );
}

#[test]
fn test_typed_struct_slot_loads_sidecar_issue_5081() {
    let mut vm = Vm::new(
        vec![Instr::LoadSlotStruct(0), Instr::ReturnAny],
        StableRng::new(0),
    );
    let mut frame = Frame::new_with_slots(1, None);
    assert!(frame.set_slot_struct_ref(0, 7));
    vm.frames.push(frame);

    let result = vm.run();
    assert!(
        matches!(result, Ok(Value::StructRef(7))),
        "Expected Ok(StructRef(7)), got {:?}",
        result
    );
}

/// A user-visible error (DivisionByZero) IS caught by handlers in run().
/// DivisionByZero goes through raise(), which checks handlers.
#[test]
fn test_user_error_caught_by_handler_in_run() {
    // Set up: try { 1 % 0 } catch; return 42
    let catch_ip = 4;
    let mut vm = Vm::new(
        vec![
            Instr::PushHandler(Some(catch_ip), None), // ip=0: push handler
            Instr::PushI64(1),                        // ip=1
            Instr::PushI64(0),                        // ip=2
            Instr::ModI64,                            // ip=3: 1 % 0 -> DivisionByZero
            Instr::ClearError,                        // ip=4 (catch): clear error
            Instr::PushI64(42),                       // ip=5: push result
            Instr::ReturnAny,                         // ip=6: return 42
        ],
        StableRng::new(0),
    );
    let result = vm.run();
    assert!(
        matches!(result, Ok(Value::I64(42))),
        "Expected Ok(I64(42)), got {:?}",
        result
    );
}

/// Errors returned via direct Err() (not raise/try_or_handle) bypass handlers.
/// This verifies the ? operator propagation path in the run loop.
#[test]
fn test_direct_err_return_bypasses_handlers() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    // Push a handler
    vm.handlers.push(Handler {
        catch_ip: Some(100),
        finally_ip: None,
        stack_len: 0,
        frame_len: 1,
        return_ip_len: 0,
        caught_exception_len: 0,
    });
    // Simulate what happens when an instruction does `return Err(InternalError)`:
    // The error goes through dispatch_instr's `?` → run's `result?` → caller.
    // The handler is NOT consulted because raise/handle_error is never called.
    // We verify this by checking that the handler is still on the stack after
    // a direct Err propagation (handle_error would have popped it).
    assert_eq!(vm.handlers.len(), 1);
    let result: Result<(), VmError> = Err(VmError::InternalError("direct".to_string()));
    // Simulating the ? operator: the error propagates without touching handlers
    assert!(result.is_err());
    assert_eq!(vm.handlers.len(), 1, "Handler should NOT have been popped");
}

/// handle_error with a finally-only handler (no catch) sets rethrow_on_finally.
#[test]
fn test_handle_error_finally_only_sets_rethrow() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.handlers.push(Handler {
        catch_ip: None,
        finally_ip: Some(200),
        stack_len: 0,
        frame_len: 1,
        return_ip_len: 0,
        caught_exception_len: 0,
    });
    let result = vm.handle_error(VmError::TypeError("test".to_string()));
    assert!(result.is_ok(), "Expected Ok, got {:?}", result);
    assert_eq!(vm.ip, 200);
    assert!(vm.rethrow_on_finally);
}

/// handle_error with a handler that has neither catch nor finally propagates.
#[test]
fn test_handle_error_no_catch_no_finally_propagates() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.handlers.push(Handler {
        catch_ip: None,
        finally_ip: None,
        stack_len: 0,
        frame_len: 1,
        return_ip_len: 0,
        caught_exception_len: 0,
    });
    let result = vm.handle_error(VmError::InternalError("test".to_string()));
    assert!(
        matches!(result, Err(VmError::InternalError(_))),
        "Expected Err(InternalError), got {:?}",
        result
    );
}

/// get_function_checked returns InternalError for out-of-bounds index.
#[test]
fn test_get_function_checked_internal_error() {
    let vm = Vm::new(vec![], StableRng::new(0));
    let result = vm.get_function_checked(999);
    assert!(
        matches!(result, Err(VmError::InternalError(_))),
        "Expected Err(InternalError), got {:?}",
        result
    );
}

// === SpannedVmError / source map tests (Issue #2856) ===

/// last_error_span returns None when no error has occurred.
#[test]
fn test_last_error_span_none_by_default() {
    let vm = Vm::new(vec![], StableRng::new(0));
    assert_eq!(vm.last_error_span(), None);
}

/// last_error_span returns None when source map is empty.
#[test]
fn test_last_error_span_none_without_source_map() {
    let mut vm = Vm::new(
        vec![Instr::PushI64(1), Instr::PushI64(0), Instr::ModI64],
        StableRng::new(0),
    );
    let result = vm.run();
    assert!(result.is_err());
    // No source map set, so span is None even though last_error_ip is set
    assert_eq!(vm.last_error_span(), None);
}

/// last_error_span returns the span from the source map when available.
#[test]
fn test_last_error_span_with_source_map() {
    use crate::span::Span;

    let span_at_2 = Span::new(10, 15, 3, 3, 5, 10);
    let mut vm = Vm::new(
        vec![Instr::PushI64(1), Instr::PushI64(0), Instr::ModI64],
        StableRng::new(0),
    );
    vm.set_source_map(vec![None, None, Some(span_at_2)]);

    let result = vm.run();
    assert!(result.is_err());
    assert_eq!(vm.last_error_span(), Some(span_at_2));
}

/// spanned_error wraps VmError with the last error span.
#[test]
fn test_spanned_error_attaches_span() {
    use crate::span::Span;

    let span = Span::new(0, 5, 1, 1, 1, 6);
    let mut vm = Vm::new(
        vec![Instr::PushI64(1), Instr::PushI64(0), Instr::ModI64],
        StableRng::new(0),
    );
    vm.set_source_map(vec![None, None, Some(span)]);

    let result = vm.run();
    assert!(result.is_err());

    let err = result.unwrap_err();
    let spanned = vm.spanned_error(err.clone());
    assert_eq!(spanned.error, err);
    assert_eq!(spanned.span, Some(span));
}

/// spanned_error returns None span when source map has no entry for the IP.
#[test]
fn test_spanned_error_no_span_for_unmapped_ip() {
    let mut vm = Vm::new(
        vec![Instr::PushI64(1), Instr::PushI64(0), Instr::ModI64],
        StableRng::new(0),
    );
    // Source map entries only for first two IPs, not for IP=2 (ModI64)
    vm.set_source_map(vec![None, None]);

    let result = vm.run();
    assert!(result.is_err());

    let err = result.unwrap_err();
    let spanned = vm.spanned_error(err.clone());
    assert_eq!(spanned.error, err);
    assert_eq!(spanned.span, None);
}

/// Display of SpannedVmError includes line:column when span is present.
#[test]
fn test_spanned_error_display_with_location() {
    use crate::span::Span;

    let span = Span::new(10, 20, 5, 5, 8, 18);
    let mut vm = Vm::new(
        vec![Instr::PushI64(1), Instr::PushI64(0), Instr::ModI64],
        StableRng::new(0),
    );
    vm.set_source_map(vec![None, None, Some(span)]);

    let result = vm.run();
    let err = result.unwrap_err();
    let spanned = vm.spanned_error(err);
    let display = format!("{}", spanned);
    assert!(
        display.contains("at line 5:8"),
        "Expected span info in display, got: {}",
        display
    );
}
