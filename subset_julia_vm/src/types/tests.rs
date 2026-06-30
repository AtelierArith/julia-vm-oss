use super::*;

#[test]
fn test_generator_type_name() {
    // JuliaType::Generator.name() should return "Base.Generator"
    assert_eq!(JuliaType::Generator.name().as_ref(), "Base.Generator");
    assert_eq!(JuliaType::Generator.to_string(), "Base.Generator");
}

#[test]
fn test_anonymous_bounded_typevar_name_issue_5644() {
    // An anonymous bounded typevar (placeholder name `_`, from the covariant
    // shorthand `Vector{<:Integer}`) prints `<:Bound`, not `_<:Bound`.
    let anon = JuliaType::TypeVar("_".to_string(), Some("Integer".to_string()));
    assert_eq!(anon.name().as_ref(), "<:Integer");

    // Rendered inside a parametric container.
    let vec_anon = JuliaType::VectorOf(Box::new(anon));
    assert_eq!(vec_anon.name().as_ref(), "Vector{<:Integer}");

    // A NAMED bounded typevar keeps its name; an unbounded one is just its name.
    let named = JuliaType::TypeVar("T".to_string(), Some("Real".to_string()));
    assert_eq!(named.name().as_ref(), "T<:Real");
    let unbounded = JuliaType::TypeVar("T".to_string(), None);
    assert_eq!(unbounded.name().as_ref(), "T");
}

#[test]
fn test_subtype_concrete() {
    // Concrete types are subtypes of themselves
    assert!(JuliaType::Int64.is_subtype_of(&JuliaType::Int64));
    assert!(JuliaType::Float64.is_subtype_of(&JuliaType::Float64));

    // Concrete types are not subtypes of other concrete types
    assert!(!JuliaType::Int64.is_subtype_of(&JuliaType::Float64));
    assert!(!JuliaType::Float64.is_subtype_of(&JuliaType::Int64));
}

#[test]
fn test_subtype_integer_hierarchy() {
    // Int64 <: Integer <: Real <: Number <: Any
    assert!(JuliaType::Int64.is_subtype_of(&JuliaType::Integer));
    assert!(JuliaType::Int64.is_subtype_of(&JuliaType::Real));
    assert!(JuliaType::Int64.is_subtype_of(&JuliaType::Number));
    assert!(JuliaType::Int64.is_subtype_of(&JuliaType::Any));

    // Integer <: Real <: Number <: Any
    assert!(JuliaType::Integer.is_subtype_of(&JuliaType::Real));
    assert!(JuliaType::Integer.is_subtype_of(&JuliaType::Number));
    assert!(JuliaType::Integer.is_subtype_of(&JuliaType::Any));
}

#[test]
fn test_array_receiver_extracts_typevar_binding_issue_4018() {
    let type_params = vec![TypeParam::new("T".to_string())];
    let array_pattern = JuliaType::Struct("Array{T}".to_string());

    let vector_bindings = JuliaType::VectorOf(Box::new(JuliaType::Int64))
        .extract_type_bindings(&array_pattern, &type_params)
        .expect("Vector{Int64} should match Array{T}");
    assert_eq!(vector_bindings.get("T"), Some(&JuliaType::Int64));

    let matrix_bindings = JuliaType::MatrixOf(Box::new(JuliaType::Float64))
        .extract_type_bindings(&array_pattern, &type_params)
        .expect("Matrix{Float64} should match Array{T}");
    assert_eq!(matrix_bindings.get("T"), Some(&JuliaType::Float64));

    let array3_bindings = JuliaType::Struct("Array{Bool, 3}".to_string())
        .extract_type_bindings(&array_pattern, &type_params)
        .expect("Array{Bool,3} should match Array{T}");
    assert_eq!(array3_bindings.get("T"), Some(&JuliaType::Bool));
}

#[test]
fn test_vector_receiver_extracts_abstract_vector_typevar_issue_8342() {
    let type_params = vec![TypeParam::new("T".to_string())];
    let pattern = JuliaType::from_name_or_struct("AbstractVector{T}");

    let bindings = JuliaType::VectorOf(Box::new(JuliaType::Float64))
        .extract_type_bindings(&pattern, &type_params)
        .expect("Vector{Float64} should match AbstractVector{T}");

    assert_eq!(bindings.get("T"), Some(&JuliaType::Float64));
}

#[test]
fn test_partial_parametric_struct_signature_extracts_prefix_issue_8348() {
    let type_params = vec![TypeParam::new("T".to_string())];
    let pattern = JuliaType::from_name_or_struct("TwoParamMatrixIssue{T}");
    let actual = JuliaType::from_name_or_struct("TwoParamMatrixIssue{Float64, Vector{Float64}}");

    assert!(
        actual.is_subtype_of_parametric(&pattern, &type_params),
        "TwoParamMatrixIssue{{T}} should match fully parameterized values"
    );

    let bindings = actual
        .extract_type_bindings(&pattern, &type_params)
        .expect("prefix parametric match should bind T");
    assert_eq!(bindings.get("T"), Some(&JuliaType::Float64));
}

#[test]
fn test_covariant_partial_parametric_struct_signature_issue_8349() {
    let pattern = JuliaType::from_name_or_struct("CovariantParamIssue{<:Real}");
    let actual = JuliaType::from_name_or_struct("CovariantParamIssue{Float64, Vector{Float64}}");
    let nonmatch = JuliaType::from_name_or_struct("CovariantParamIssue{String, Vector{String}}");

    assert!(
        actual.is_subtype_of_parametric(&pattern, &[]),
        "CovariantParamIssue{{<:Real}} should match a Float64 first parameter"
    );
    assert!(
        actual.extract_type_bindings(&pattern, &[]).is_some(),
        "covariant prefix match should be accepted without bindings"
    );
    assert!(
        !nonmatch.is_subtype_of_parametric(&pattern, &[]),
        "CovariantParamIssue{{<:Real}} must reject non-Real first parameters"
    );
}

#[test]
fn test_array_receiver_extracts_nested_pair_bindings_issue_4635() {
    let type_params = vec![
        TypeParam::new("K".to_string()),
        TypeParam::new("V".to_string()),
    ];
    let array_pattern = JuliaType::Struct("Array{Pair{K,V}}".to_string());
    let array_actual = JuliaType::Struct("Array{Pair{Int64,Int8}}".to_string());

    let bindings = array_actual
        .extract_type_bindings(&array_pattern, &type_params)
        .expect("Array{Pair{Int64,Int8}} should match Array{Pair{K,V}}");

    assert_eq!(bindings.get("K"), Some(&JuliaType::Int64));
    assert_eq!(bindings.get("V"), Some(&JuliaType::Int8));
}

#[test]
fn test_matrix_covariant_bound_parametric_dispatch_issue_4020() {
    let matrix_integer = JuliaType::from_name_or_struct("Matrix{<:Integer}");
    let matrix_int = JuliaType::MatrixOf(Box::new(JuliaType::Int64));
    let matrix_float = JuliaType::MatrixOf(Box::new(JuliaType::Float64));

    assert!(
        matrix_int.is_subtype_of_parametric(&matrix_integer, &[]),
        "Matrix{{Int64}} should match Matrix{{<:Integer}}"
    );
    assert!(
        !matrix_float.is_subtype_of_parametric(&matrix_integer, &[]),
        "Matrix{{Float64}} must not match Matrix{{<:Integer}}"
    );
}

#[test]
fn test_subtype_float_hierarchy() {
    // Float64 <: AbstractFloat <: Real <: Number <: Any
    assert!(JuliaType::Float64.is_subtype_of(&JuliaType::AbstractFloat));
    assert!(JuliaType::Float64.is_subtype_of(&JuliaType::Real));
    assert!(JuliaType::Float64.is_subtype_of(&JuliaType::Number));
    assert!(JuliaType::Float64.is_subtype_of(&JuliaType::Any));
}

// Note: Complex is now a user-defined struct, tested separately

#[test]
fn test_subtype_string() {
    // String <: AbstractString <: Any
    assert!(JuliaType::String.is_subtype_of(&JuliaType::AbstractString));
    assert!(JuliaType::String.is_subtype_of(&JuliaType::Any));
    assert!(!JuliaType::String.is_subtype_of(&JuliaType::Number));
}

#[test]
fn test_subtype_array() {
    // Array <: AbstractArray <: Any
    assert!(JuliaType::Array.is_subtype_of(&JuliaType::AbstractArray));
    assert!(JuliaType::Array.is_subtype_of(&JuliaType::Any));
    assert!(!JuliaType::Array.is_subtype_of(&JuliaType::Number));
}

#[test]
fn test_abstract_user_any_parent_not_universal_supertype_issue_4708() {
    // Issue #4708 (regression guard, Issue #4710): before the fix, the
    // AbstractUser parent fallback evaluated `self.is_subtype_of(parent)`
    // even when parent was `Any`. Because every type is <: Any, this
    // made every value spuriously match user-declared abstracts whose
    // parent chain bottomed out at Any (e.g. AbstractDict and
    // AbstractSet in boot.jl). The fix skips the recursive fallback
    // when parent == "Any" and defers to CoreType for builtin abstract
    // names instead. This test matrix locks the regression in place.

    // The way Pure Julia's `boot.jl` declares these abstracts.
    let abstract_dict =
        JuliaType::AbstractUser("AbstractDict".to_string(), Some("Any".to_string()));
    let abstract_set = JuliaType::AbstractUser("AbstractSet".to_string(), Some("Any".to_string()));
    let some_user_abstract_any =
        JuliaType::AbstractUser("MyAbstractAny".to_string(), Some("Any".to_string()));

    // Negative cases — these MUST NOT be subtypes of the abstracts
    // above. Before #4710 the parent-Any fallback returned true for
    // every entry here.
    let not_container_like = [
        JuliaType::Array,
        JuliaType::VectorOf(Box::new(JuliaType::Int64)),
        JuliaType::MatrixOf(Box::new(JuliaType::Float64)),
        JuliaType::Tuple,
        JuliaType::TupleOf(vec![JuliaType::Int64, JuliaType::String]),
        JuliaType::Int64,
        JuliaType::Float64,
        JuliaType::String,
        JuliaType::Symbol,
        JuliaType::Char,
        JuliaType::Bool,
        JuliaType::Nothing,
    ];
    for ty in &not_container_like {
        assert!(
            !ty.is_subtype_of(&abstract_dict),
            "{ty:?} must not be <: AbstractDict (Issue #4708)"
        );
        assert!(
            !ty.is_subtype_of(&abstract_set),
            "{ty:?} must not be <: AbstractSet (Issue #4708)"
        );
        assert!(
            !ty.is_subtype_of(&some_user_abstract_any),
            "{ty:?} must not be <: a user abstract whose parent is Any (Issue #4708)"
        );
    }

    // Positive cases — builtin containers must keep matching the
    // CoreType-backed abstracts they belong to.
    assert!(JuliaType::Dict.is_subtype_of(&abstract_dict));
    assert!(JuliaType::Set.is_subtype_of(&abstract_set));
    assert!(JuliaType::Struct("Dict{String, Int64}".to_string()).is_subtype_of(&abstract_dict));
    assert!(JuliaType::Struct("Set{Int64}".to_string()).is_subtype_of(&abstract_set));

    // Self-referential and AbstractUser parent matching still work.
    let myabs_dict_child = JuliaType::AbstractUser(
        "MyAbstractDictSub".to_string(),
        Some("AbstractDict".to_string()),
    );
    assert!(myabs_dict_child.is_subtype_of(&abstract_dict));
    assert!(abstract_dict.is_subtype_of(&abstract_dict));
}

#[test]
fn test_abstract_user_specific_parent_fallback_preserved_issue_4708() {
    // Issue #4708: the fallback `self.is_subtype_of(parent)` is *kept*
    // for CoreType-backed abstract names, so hierarchies like AbstractVector
    // (parent="AbstractArray") still recognise VectorOf as a matching
    // candidate without treating rank-unknown Array as every vector. Without this, fixtures such as
    // matmul_matrix_matrix_abstract_dispatch_4020 and
    // nullspace_logdet_adjoint would silently regress.
    let abstract_vector = JuliaType::AbstractUser(
        "AbstractVector".to_string(),
        Some("AbstractArray".to_string()),
    );
    assert!(JuliaType::VectorOf(Box::new(JuliaType::Int64)).is_subtype_of(&abstract_vector));
    assert!(!JuliaType::Array.is_subtype_of(&abstract_vector));
}

#[test]
fn test_subtype_structured_builtin_families_use_core_type() {
    assert!(JuliaType::Struct("StepRangeLen{Float64}".to_string())
        .is_subtype_of(&JuliaType::AbstractRange));
    assert!(
        JuliaType::AbstractRange.is_subtype_of(&JuliaType::Struct("AbstractVector".to_string()))
    );
    assert!(JuliaType::AbstractRange.is_subtype_of(&JuliaType::AbstractArray));
    assert!(JuliaType::Struct("UnitRange{Int64}".to_string())
        .is_subtype_of(&JuliaType::Struct("AbstractVector{Int64}".to_string())));
    assert!(JuliaType::Struct("UnitRange{Int64}".to_string())
        .is_subtype_of(&JuliaType::Struct("AbstractArray{Int64,1}".to_string())));
    assert!(!JuliaType::Struct("UnitRange{Int64}".to_string())
        .is_subtype_of(&JuliaType::Struct("AbstractVector{Integer}".to_string())));
    assert!(!JuliaType::Struct("UnitRange{Int64}".to_string())
        .is_subtype_of(&JuliaType::Struct("Array{Int64,1}".to_string())));
    assert!(JuliaType::Struct("LogRange{Float64}".to_string())
        .is_subtype_of(&JuliaType::Struct("AbstractVector{Float64}".to_string())));
    assert!(JuliaType::Struct("LogRange{Float64}".to_string())
        .is_subtype_of(&JuliaType::Struct("AbstractArray{Float64,1}".to_string())));
    assert!(!JuliaType::Struct("LogRange{Float64}".to_string())
        .is_subtype_of(&JuliaType::AbstractRange));
    assert!(JuliaType::Struct("IOBuffer".to_string()).is_subtype_of(&JuliaType::IO));
    assert!(JuliaType::Struct("Vector{Int64}".to_string()).is_subtype_of(&JuliaType::Array));
    assert!(JuliaType::Struct("Tuple{Int64, String}".to_string()).is_subtype_of(&JuliaType::Tuple));
}

#[test]
fn test_specificity() {
    // Concrete > specific abstract > general abstract > Any
    assert!(JuliaType::Int64.specificity() > JuliaType::Integer.specificity());
    assert!(JuliaType::Integer.specificity() > JuliaType::Real.specificity());
    assert!(JuliaType::Real.specificity() > JuliaType::Number.specificity());
    assert!(JuliaType::Number.specificity() > JuliaType::Any.specificity());
}

/// Test that TupleOf specificity uses element-wise sum scoring (Issue #2302, #2321).
///
/// Tuple{Int64, Int64} must be more specific than Tuple{Int64, Any},
/// which must be more specific than Tuple{Any, Any}.
/// The correct approach is sum() of element specificities.
#[test]
fn test_tuple_of_specificity_ordering() {
    // 2-element tuple specificity ordering
    let any_any = JuliaType::TupleOf(vec![JuliaType::Any, JuliaType::Any]);
    let int_any = JuliaType::TupleOf(vec![JuliaType::Int64, JuliaType::Any]);
    let any_int = JuliaType::TupleOf(vec![JuliaType::Any, JuliaType::Int64]);
    let int_int = JuliaType::TupleOf(vec![JuliaType::Int64, JuliaType::Int64]);

    // Fully concrete > partially concrete > fully abstract
    assert!(
        int_int.specificity() > int_any.specificity(),
        "Tuple{{Int64, Int64}} should be more specific than Tuple{{Int64, Any}}"
    );
    assert!(
        int_any.specificity() > any_any.specificity(),
        "Tuple{{Int64, Any}} should be more specific than Tuple{{Any, Any}}"
    );
    assert!(
        any_int.specificity() > any_any.specificity(),
        "Tuple{{Any, Int64}} should be more specific than Tuple{{Any, Any}}"
    );

    // int_any and any_int should have equal specificity (same sum)
    assert_eq!(
        int_any.specificity(),
        any_int.specificity(),
        "Tuple{{Int64, Any}} and Tuple{{Any, Int64}} should have equal specificity"
    );

    // 3-element tuple specificity ordering (Issue #2321 prevention test)
    let three_any = JuliaType::TupleOf(vec![JuliaType::Any, JuliaType::Any, JuliaType::Any]);
    let int_any_any = JuliaType::TupleOf(vec![JuliaType::Int64, JuliaType::Any, JuliaType::Any]);
    let int_int_any = JuliaType::TupleOf(vec![JuliaType::Int64, JuliaType::Int64, JuliaType::Any]);
    let int_int_int =
        JuliaType::TupleOf(vec![JuliaType::Int64, JuliaType::Int64, JuliaType::Int64]);

    // Strict ordering: int_int_int > int_int_any > int_any_any > three_any
    assert!(
        int_int_int.specificity() > int_int_any.specificity(),
        "Tuple{{Int64, Int64, Int64}} > Tuple{{Int64, Int64, Any}}"
    );
    assert!(
        int_int_any.specificity() > int_any_any.specificity(),
        "Tuple{{Int64, Int64, Any}} > Tuple{{Int64, Any, Any}}"
    );
    assert!(
        int_any_any.specificity() > three_any.specificity(),
        "Tuple{{Int64, Any, Any}} > Tuple{{Any, Any, Any}}"
    );

    // Empty tuple should have concrete specificity
    let empty_tuple = JuliaType::TupleOf(vec![]);
    assert_eq!(
        empty_tuple.specificity(),
        5,
        "Empty tuple should have concrete specificity"
    );

    // Single element tuples
    let single_any = JuliaType::TupleOf(vec![JuliaType::Any]);
    let single_int = JuliaType::TupleOf(vec![JuliaType::Int64]);
    assert!(
        single_int.specificity() > single_any.specificity(),
        "Tuple{{Int64}} > Tuple{{Any}}"
    );
}

/// Test that TupleOf with varying element types produces correct specificity.
/// This ensures mixed concrete types don't accidentally score differently.
#[test]
fn test_tuple_of_specificity_mixed_types() {
    // All concrete types should contribute equally to specificity
    let int_int = JuliaType::TupleOf(vec![JuliaType::Int64, JuliaType::Int64]);
    let int_float = JuliaType::TupleOf(vec![JuliaType::Int64, JuliaType::Float64]);
    let str_bool = JuliaType::TupleOf(vec![JuliaType::String, JuliaType::Bool]);

    // All fully-concrete 2-tuples should have the same specificity
    assert_eq!(
        int_int.specificity(),
        int_float.specificity(),
        "All concrete 2-tuples should have equal specificity"
    );
    assert_eq!(
        int_float.specificity(),
        str_bool.specificity(),
        "All concrete 2-tuples should have equal specificity"
    );

    // Abstract elements reduce specificity
    let int_number = JuliaType::TupleOf(vec![JuliaType::Int64, JuliaType::Number]);
    let int_real = JuliaType::TupleOf(vec![JuliaType::Int64, JuliaType::Real]);

    // Concrete > more specific abstract > less specific abstract
    assert!(
        int_int.specificity() > int_real.specificity(),
        "Tuple{{Int64, Int64}} > Tuple{{Int64, Real}}"
    );
    assert!(
        int_real.specificity() > int_number.specificity(),
        "Tuple{{Int64, Real}} > Tuple{{Int64, Number}}"
    );
}

/// Test that VectorOf/MatrixOf specificity uses element-type scoring (Issue #2352).
///
/// Vector{Int64} must be more specific than Vector{Any}.
#[test]
fn test_vector_of_specificity() {
    let vec_int64 = JuliaType::VectorOf(Box::new(JuliaType::Int64));
    let vec_float64 = JuliaType::VectorOf(Box::new(JuliaType::Float64));
    let vec_number = JuliaType::VectorOf(Box::new(JuliaType::Number));
    let vec_real = JuliaType::VectorOf(Box::new(JuliaType::Real));
    let vec_any = JuliaType::VectorOf(Box::new(JuliaType::Any));

    // Concrete element types should have higher specificity than abstract
    assert!(
        vec_int64.specificity() > vec_any.specificity(),
        "Vector{{Int64}} > Vector{{Any}}"
    );
    assert!(
        vec_float64.specificity() > vec_any.specificity(),
        "Vector{{Float64}} > Vector{{Any}}"
    );

    // All concrete element types should have equal specificity
    assert_eq!(
        vec_int64.specificity(),
        vec_float64.specificity(),
        "Vector{{Int64}} == Vector{{Float64}} (both concrete)"
    );

    // Specificity follows element type hierarchy
    assert!(
        vec_int64.specificity() > vec_real.specificity(),
        "Vector{{Int64}} > Vector{{Real}}"
    );
    assert!(
        vec_real.specificity() > vec_number.specificity(),
        "Vector{{Real}} > Vector{{Number}}"
    );
    assert!(
        vec_number.specificity() > vec_any.specificity(),
        "Vector{{Number}} > Vector{{Any}}"
    );
}

/// Test that MatrixOf specificity uses element-type scoring (Issue #2352).
#[test]
fn test_matrix_of_specificity() {
    let mat_int64 = JuliaType::MatrixOf(Box::new(JuliaType::Int64));
    let mat_float64 = JuliaType::MatrixOf(Box::new(JuliaType::Float64));
    let mat_any = JuliaType::MatrixOf(Box::new(JuliaType::Any));

    // Concrete element types should have higher specificity than abstract
    assert!(
        mat_int64.specificity() > mat_any.specificity(),
        "Matrix{{Int64}} > Matrix{{Any}}"
    );

    // All concrete element types should have equal specificity
    assert_eq!(
        mat_int64.specificity(),
        mat_float64.specificity(),
        "Matrix{{Int64}} == Matrix{{Float64}} (both concrete)"
    );
}

#[test]
fn test_from_name() {
    assert_eq!(JuliaType::from_name("Int64"), Some(JuliaType::Int64));
    assert_eq!(
        JuliaType::from_name("Int"),
        Some(crate::types::native_int_julia_type())
    );
    assert_eq!(JuliaType::from_name("Float64"), Some(JuliaType::Float64));
    assert_eq!(
        JuliaType::from_name("ComplexF64"),
        Some(JuliaType::Struct("Complex{Float64}".to_string()))
    );
    assert_eq!(
        JuliaType::from_name("ComplexF32"),
        Some(JuliaType::Struct("Complex{Float32}".to_string()))
    );
    assert_eq!(JuliaType::from_name("Number"), Some(JuliaType::Number));
    assert_eq!(JuliaType::from_name("Any"), Some(JuliaType::Any));
    assert_eq!(JuliaType::from_name("UnknownType"), None);
}

/// Test that `from_name()` correctly parses parametric tuple type strings
/// into `JuliaType::TupleOf(...)` (Issue #1752).
///
/// This prevents regressions in parametric tuple dispatch (Issue #1748)
/// where `Tuple{Int64, String}` was not recognized as a parametric type.
#[test]
fn test_from_name_parametric_tuple() {
    // Basic parametric tuple types
    assert_eq!(
        JuliaType::from_name("Tuple{Int64, String}"),
        Some(JuliaType::TupleOf(vec![
            JuliaType::Int64,
            JuliaType::String
        ]))
    );
    assert_eq!(
        JuliaType::from_name("Tuple{Int64, Int64}"),
        Some(JuliaType::TupleOf(vec![JuliaType::Int64, JuliaType::Int64]))
    );
    assert_eq!(
        JuliaType::from_name("Tuple{Float64}"),
        Some(JuliaType::TupleOf(vec![JuliaType::Float64]))
    );

    // Tuple with Union element types
    assert_eq!(
        JuliaType::from_name("Tuple{Union{Int64, String}, Float64}"),
        Some(JuliaType::TupleOf(vec![
            JuliaType::Union(vec![JuliaType::Int64, JuliaType::String]),
            JuliaType::Float64,
        ]))
    );

    // Tuple with Nothing element. The Union is canonicalized (Issue #5066):
    // `Nothing` is a singleton DataType and so sorts ahead of the `isbits`
    // `Int64`, matching upstream's `Union{Nothing, Int64}`.
    assert_eq!(
        JuliaType::from_name("Tuple{Union{Int64, Nothing}, String}"),
        Some(JuliaType::TupleOf(vec![
            JuliaType::Union(vec![JuliaType::Nothing, JuliaType::Int64]),
            JuliaType::String,
        ]))
    );

    // Issue #8360: user-parametric members inside a Union alias must remain
    // nominal members, not collapse to Any during parsing/canonicalization.
    let user_union = JuliaType::from_name_or_struct("Union{H{T}, S{T}}");
    match user_union {
        JuliaType::Union(members) => {
            assert_eq!(members.len(), 2);
            assert!(members.contains(&JuliaType::Struct("H{T}".to_string())));
            assert!(members.contains(&JuliaType::Struct("S{T}".to_string())));
        }
        other => panic!("expected Union, got {other:?}"),
    }

    let actual = JuliaType::Struct("M.H{Float64, Vector{Float64}}".to_string());
    let pattern = JuliaType::from_name_or_struct("Union{H{T}, S{T}}");
    let params = vec![TypeParam::with_upper_bound(
        "T".to_string(),
        "Real".to_string(),
    )];
    let bindings = actual
        .extract_type_bindings(&pattern, &params)
        .expect("Union alias arm should bind T from matching member");
    assert_eq!(bindings.get("T"), Some(&JuliaType::Float64));

    // Tuple with Any element
    assert_eq!(
        JuliaType::from_name("Tuple{Any, Any}"),
        Some(JuliaType::TupleOf(vec![JuliaType::Any, JuliaType::Any]))
    );

    // Empty Tuple{} is the concrete empty tuple type, distinct from abstract Tuple.
    assert_eq!(
        JuliaType::from_name("Tuple{}"),
        Some(JuliaType::TupleOf(vec![]))
    );

    // Plain Tuple (no braces) should return Tuple
    assert_eq!(JuliaType::from_name("Tuple"), Some(JuliaType::Tuple));

    // Fixed NTuple aliases should canonicalize to expanded TupleOf for
    // VM-facing equality/isa checks (Issue #4281).
    assert_eq!(
        JuliaType::from_name("NTuple{3, Int64}"),
        Some(JuliaType::TupleOf(vec![
            JuliaType::Int64,
            JuliaType::Int64,
            JuliaType::Int64,
        ]))
    );
    assert_eq!(
        JuliaType::from_name("NTuple{2, String}"),
        Some(JuliaType::TupleOf(vec![
            JuliaType::String,
            JuliaType::String
        ]))
    );

    // Nested parametric tuple: Tuple{Tuple{Int64}, String}
    assert_eq!(
        JuliaType::from_name("Tuple{Tuple{Int64}, String}"),
        Some(JuliaType::TupleOf(vec![
            JuliaType::TupleOf(vec![JuliaType::Int64]),
            JuliaType::String,
        ]))
    );

    // Tuple with Bool element
    assert_eq!(
        JuliaType::from_name("Tuple{Bool, Int64, String}"),
        Some(JuliaType::TupleOf(vec![
            JuliaType::Bool,
            JuliaType::Int64,
            JuliaType::String,
        ]))
    );
}

/// Type-level `NamedTuple{names, T}` canonicalization (Issue #5063).
///
/// The upstream spelling must canonicalize to the same internal representation
/// `typeof((a=1, b=2))` and the `@NamedTuple` macro produce, so subtype / isa /
/// dispatch / `===` reuse the existing named-tuple machinery.
#[test]
fn test_from_name_named_tuple_type_level() {
    // Names + field-type tuple -> concrete `@NamedTuple{...}` form (Int -> Int64).
    assert_eq!(
        JuliaType::from_name("NamedTuple{(:a,:b),Tuple{Int,Int}}"),
        Some(JuliaType::Struct(
            "@NamedTuple{a::Int64, b::Int64}".to_string()
        ))
    );
    // Whitespace in the spelling is tolerated and the field types canonicalize.
    assert_eq!(
        JuliaType::from_name("NamedTuple{(:a, :b), Tuple{Int, Float64}}"),
        Some(JuliaType::Struct(
            "@NamedTuple{a::Int64, b::Float64}".to_string()
        ))
    );
    // An `Any`-typed field collapses to the bare name, matching upstream printing.
    assert_eq!(
        JuliaType::from_name("NamedTuple{(:a,:b),Tuple{Int,Any}}"),
        Some(JuliaType::Struct("@NamedTuple{a::Int64, b}".to_string()))
    );
    // Single-field concrete form keeps the field type.
    assert_eq!(
        JuliaType::from_name("NamedTuple{(:x,),Tuple{Int64}}"),
        Some(JuliaType::Struct("@NamedTuple{x::Int64}".to_string()))
    );

    // Names-only form -> the `NamedTuple{(:a, :b)}` UnionAll-style marker with
    // canonical spacing (single field carries a trailing comma).
    assert_eq!(
        JuliaType::from_name("NamedTuple{(:a,:b)}"),
        Some(JuliaType::Struct("NamedTuple{(:a, :b)}".to_string()))
    );
    assert_eq!(
        JuliaType::from_name("NamedTuple{(:x,)}"),
        Some(JuliaType::Struct("NamedTuple{(:x,)}".to_string()))
    );

    // Bare `NamedTuple` is unchanged.
    assert_eq!(
        JuliaType::from_name("NamedTuple"),
        Some(JuliaType::NamedTuple)
    );

    // Arity mismatch between names and field types is not a well-formed concrete
    // named tuple; falls back to `None` (struct fallback handled by the caller).
    assert_eq!(JuliaType::from_name("NamedTuple{(:a,:b),Tuple{Int}}"), None);

    // The concrete form is a subtype of the names-only marker and of the bare
    // `NamedTuple`, mirroring upstream's `<:` relationships.
    let concrete = JuliaType::from_name("NamedTuple{(:a,:b),Tuple{Int,Int}}").unwrap();
    let names_only = JuliaType::from_name("NamedTuple{(:a,:b)}").unwrap();
    assert!(concrete.is_subtype_of(&names_only));
    assert!(concrete.is_subtype_of(&JuliaType::NamedTuple));
    assert!(names_only.is_subtype_of(&JuliaType::NamedTuple));
    // A different field-name set is NOT a subtype of the marker.
    let other = JuliaType::from_name("NamedTuple{(:x,:y),Tuple{Int,Int}}").unwrap();
    assert!(!other.is_subtype_of(&names_only));
}

/// Test covariant subtyping for parametric tuple types (Issue #1752).
///
/// In Julia, tuples are covariant: Tuple{Int64} <: Tuple{Number}.
/// This is essential for parametric tuple dispatch to work correctly.
#[test]
fn test_tuple_of_subtyping() {
    // Tuple{Int64, Int64} <: Tuple{Number, Number} (covariant)
    let concrete = JuliaType::TupleOf(vec![JuliaType::Int64, JuliaType::Int64]);
    let abstract_tup = JuliaType::TupleOf(vec![JuliaType::Number, JuliaType::Number]);
    assert!(concrete.is_subtype_of(&abstract_tup));

    // Tuple{Int64, String} <: Tuple{Any, Any}
    let mixed = JuliaType::TupleOf(vec![JuliaType::Int64, JuliaType::String]);
    let any_tup = JuliaType::TupleOf(vec![JuliaType::Any, JuliaType::Any]);
    assert!(mixed.is_subtype_of(&any_tup));

    // Tuple{Int64} is NOT a subtype of Tuple{Int64, Int64} (length mismatch)
    let short = JuliaType::TupleOf(vec![JuliaType::Int64]);
    let long = JuliaType::TupleOf(vec![JuliaType::Int64, JuliaType::Int64]);
    assert!(!short.is_subtype_of(&long));
    assert!(!long.is_subtype_of(&short));

    // TupleOf <: Tuple (parametric is subtype of non-parametric)
    let parametric = JuliaType::TupleOf(vec![JuliaType::Int64, JuliaType::String]);
    assert!(parametric.is_subtype_of(&JuliaType::Tuple));

    // Tuple is NOT a subtype of TupleOf (non-parametric is too general)
    assert!(!JuliaType::Tuple.is_subtype_of(&parametric));

    // Tuple{Int64, Int64} <: Tuple{Union{Int64, String}, Int64}
    let concrete_pair = JuliaType::TupleOf(vec![JuliaType::Int64, JuliaType::Int64]);
    let union_param = JuliaType::TupleOf(vec![
        JuliaType::Union(vec![JuliaType::Int64, JuliaType::String]),
        JuliaType::Int64,
    ]);
    assert!(concrete_pair.is_subtype_of(&union_param));
}

#[test]
fn test_union_type_eq_is_order_insensitive() {
    let int_string = JuliaType::Union(vec![JuliaType::Int64, JuliaType::String]);
    let string_int = JuliaType::Union(vec![JuliaType::String, JuliaType::Int64]);
    assert!(int_string.type_eq(&string_int));

    let tuple_left = JuliaType::TupleOf(vec![int_string]);
    let tuple_right = JuliaType::TupleOf(vec![string_int]);
    assert!(tuple_left.type_eq(&tuple_right));
}

#[test]
fn test_module_struct_type_eq_ignores_imported_binding_prefix_issue_4348() {
    let qualified = JuliaType::Struct("TestModule.MyStruct{Int64}".to_string());
    let imported = JuliaType::Struct("MyStruct{Int64}".to_string());
    assert!(qualified.type_eq(&imported));
    assert!(imported.type_eq(&qualified));
}

#[test]
fn test_is_concrete() {
    assert!(JuliaType::Int64.is_concrete());
    assert!(JuliaType::Float64.is_concrete());
    assert!(JuliaType::String.is_concrete());
    assert!(JuliaType::Array.is_concrete());
    assert!(JuliaType::Struct("Point".to_string()).is_concrete());
    assert!(JuliaType::Struct("Complex".to_string()).is_concrete()); // Complex is now a struct
    assert!(JuliaType::Struct("Complex{Float64}".to_string()).is_concrete());
    assert!(JuliaType::TupleOf(vec![JuliaType::Int64, JuliaType::String]).is_concrete());
    assert!(JuliaType::TypeOf(Box::new(JuliaType::Int64)).is_concrete());

    assert!(!JuliaType::Any.is_concrete());
    assert!(!JuliaType::Number.is_concrete());
    assert!(!JuliaType::Real.is_concrete());
    assert!(!JuliaType::Union(vec![JuliaType::Int64, JuliaType::Float64]).is_concrete());
    assert!(!JuliaType::Struct("Complex{T}".to_string()).is_concrete());
    assert!(!JuliaType::TupleOf(vec![JuliaType::TypeVar("T".to_string(), None)]).is_concrete());
    assert!(!JuliaType::UnionAll {
        var: "T".to_string(),
        lower_bound: None,
        bound: None,
        body: Box::new(JuliaType::VectorOf(Box::new(JuliaType::TypeVar(
            "T".to_string(),
            None
        )))),
    }
    .is_concrete());
}

#[test]
fn test_struct_type() {
    let point = JuliaType::Struct("Point".to_string());
    let vector3d = JuliaType::Struct("Vector3D".to_string());

    // Struct is subtype of Any
    assert!(point.is_subtype_of(&JuliaType::Any));

    // Struct is subtype of itself (same name)
    assert!(point.is_subtype_of(&point));
    assert!(point.is_subtype_of(&JuliaType::Struct("Point".to_string())));

    // Different struct names are not subtypes of each other
    assert!(!point.is_subtype_of(&vector3d));
    assert!(!vector3d.is_subtype_of(&point));

    // Struct is not subtype of Number, Real, etc.
    assert!(!point.is_subtype_of(&JuliaType::Number));
    assert!(!point.is_subtype_of(&JuliaType::Real));

    // Struct has highest specificity
    assert_eq!(point.specificity(), JuliaType::Int64.specificity());

    // from_name_or_struct treats unknown names as structs
    assert_eq!(
        JuliaType::from_name_or_struct("Point"),
        JuliaType::Struct("Point".to_string())
    );
    assert_eq!(JuliaType::from_name_or_struct("Int64"), JuliaType::Int64);
}

#[test]
fn test_module_qualified_parametric_struct_subtypes_bare_family_issue_6117() {
    let actual = JuliaType::from_name_or_struct("LinearAlgebra.Diagonal{Float64}");
    let bare = JuliaType::from_name_or_struct("Diagonal");
    let same_param = JuliaType::from_name_or_struct("Diagonal{Float64}");
    let other_param = JuliaType::from_name_or_struct("Diagonal{Int64}");

    assert!(actual.is_subtype_of(&bare));
    assert!(actual.is_subtype_of(&same_param));
    assert!(!actual.is_subtype_of(&other_param));
}

/// Comprehensive test to ensure all builtin types with standard names have
/// proper `from_name()` mappings.
///
/// This test prevents bugs like Issue #1328 where a missing `from_name()` mapping
/// caused `f::Function` to be incorrectly treated as a user-defined struct.
///
/// When adding new JuliaType variants, add them to this test.
#[test]
fn test_from_name_builtin_coverage() {
    // All builtin types that should be parseable by their standard Julia name
    let expected_mappings: &[(&str, JuliaType)] = &[
        // Signed integers
        ("Int8", JuliaType::Int8),
        ("Int16", JuliaType::Int16),
        ("Int32", JuliaType::Int32),
        ("Int64", JuliaType::Int64),
        ("Int", JuliaType::Int64), // Alias
        ("Int128", JuliaType::Int128),
        ("BigInt", JuliaType::BigInt),
        // Unsigned integers
        ("UInt8", JuliaType::UInt8),
        ("UInt16", JuliaType::UInt16),
        ("UInt32", JuliaType::UInt32),
        ("UInt64", JuliaType::UInt64),
        ("UInt", JuliaType::UInt64), // Alias
        ("UInt128", JuliaType::UInt128),
        // Boolean
        ("Bool", JuliaType::Bool),
        // Floating point
        ("Float16", JuliaType::Float16),
        ("Float32", JuliaType::Float32),
        ("Float64", JuliaType::Float64),
        ("BigFloat", JuliaType::BigFloat),
        // String/Char
        ("String", JuliaType::String),
        ("Char", JuliaType::Char),
        // Collections
        ("Array", JuliaType::Array),
        ("Vector", JuliaType::Struct("Vector".to_string())),
        ("Matrix", JuliaType::Struct("Matrix".to_string())),
        ("BitArray", JuliaType::Struct("BitArray".to_string())),
        ("BitVector", JuliaType::Struct("BitVector".to_string())),
        ("BitMatrix", JuliaType::Struct("BitMatrix".to_string())),
        ("Tuple", JuliaType::Tuple),
        ("NamedTuple", JuliaType::NamedTuple),
        ("Dict", JuliaType::Dict),
        ("Dictionary", JuliaType::Dict), // Alias
        ("Set", JuliaType::Set),
        // Range types
        ("UnitRange", JuliaType::UnitRange),
        ("StepRange", JuliaType::StepRange),
        // Special types
        ("Nothing", JuliaType::Nothing),
        ("Missing", JuliaType::Missing),
        // Abstract types
        ("Any", JuliaType::Any),
        ("Number", JuliaType::Number),
        ("Real", JuliaType::Real),
        ("Integer", JuliaType::Integer),
        ("Signed", JuliaType::Signed),
        ("Unsigned", JuliaType::Unsigned),
        ("AbstractFloat", JuliaType::AbstractFloat),
        ("AbstractString", JuliaType::AbstractString),
        ("AbstractChar", JuliaType::AbstractChar),
        ("AbstractArray", JuliaType::AbstractArray),
        (
            "AbstractVector",
            JuliaType::Struct("AbstractVector".to_string()),
        ),
        (
            "AbstractMatrix",
            JuliaType::Struct("AbstractMatrix".to_string()),
        ),
        ("AbstractRange", JuliaType::AbstractRange),
        // Type system types
        ("IO", JuliaType::IO),
        ("IOBuffer", JuliaType::IOBuffer),
        ("Module", JuliaType::Module),
        ("Type", JuliaType::Type),
        ("DataType", JuliaType::DataType),
        // Macro system types
        ("Symbol", JuliaType::Symbol),
        ("Expr", JuliaType::Expr),
        ("QuoteNode", JuliaType::QuoteNode),
        ("LineNumberNode", JuliaType::LineNumberNode),
        ("GlobalRef", JuliaType::GlobalRef),
        // Function type (Issue #1328 - this was missing!)
        ("Function", JuliaType::Function),
        // Bottom type
        ("Union{}", JuliaType::Bottom),
        ("Bottom", JuliaType::Bottom),
    ];

    for (name, expected) in expected_mappings {
        let actual = JuliaType::from_name(name);
        assert_eq!(
            actual,
            Some(expected.clone()),
            "from_name({:?}) should return Some({:?}), but got {:?}. \
             If you added a new JuliaType variant, add it to the match arms in from_name().",
            name,
            expected,
            actual
        );
    }
}

/// Exhaustive test for the `is_type_variable_name` heuristic (Issue #2273).
/// This ensures the heuristic correctly distinguishes type variables from
/// concrete type names without semantic context from `where` clauses.
#[test]
fn test_is_type_variable_name() {
    // Single uppercase letter — standard type variable names
    assert!(is_type_variable_name("T"));
    assert!(is_type_variable_name("S"));
    assert!(is_type_variable_name("A"));
    assert!(is_type_variable_name("B"));
    assert!(is_type_variable_name("N"));
    assert!(is_type_variable_name("X"));

    // Uppercase letter + digits — multi-parameter where clauses (Issue #2248)
    assert!(is_type_variable_name("T1"));
    assert!(is_type_variable_name("T2"));
    assert!(is_type_variable_name("T3"));
    assert!(is_type_variable_name("T10"));
    assert!(is_type_variable_name("T20"));
    assert!(is_type_variable_name("S1"));
    assert!(is_type_variable_name("A1"));
    assert!(is_type_variable_name("N1"));

    // Should NOT match concrete Julia type names
    assert!(!is_type_variable_name("Int64"));
    assert!(!is_type_variable_name("Float32"));
    assert!(!is_type_variable_name("Float16"));
    assert!(!is_type_variable_name("Bool"));
    assert!(!is_type_variable_name("Number"));
    assert!(!is_type_variable_name("Real"));
    assert!(!is_type_variable_name("String"));
    assert!(!is_type_variable_name("Array"));
    assert!(!is_type_variable_name("UInt8"));
    assert!(!is_type_variable_name("Int128"));

    // Should NOT match lowercase or empty strings
    assert!(!is_type_variable_name(""));
    assert!(!is_type_variable_name("t"));
    assert!(!is_type_variable_name("abc"));
    assert!(!is_type_variable_name("type"));

    // Should NOT match digit-first strings
    assert!(!is_type_variable_name("1T"));
    assert!(!is_type_variable_name("123"));

    // Multi-uppercase-letter names are NOT type variables (they're struct names)
    assert!(!is_type_variable_name("TT"));
    assert!(!is_type_variable_name("TS"));
    assert!(!is_type_variable_name("IO"));
    assert!(!is_type_variable_name("Any"));
}

/// Test that unknown type names return None (not incorrectly parsed as builtins)
#[test]
fn test_from_name_unknown_types() {
    // User-defined struct names should return None
    assert_eq!(JuliaType::from_name("Point"), None);
    assert_eq!(JuliaType::from_name("MyStruct"), None);
    assert_eq!(JuliaType::from_name("Complex"), None); // Complex is now a Pure Julia struct
    assert_eq!(JuliaType::from_name("Rational"), None);

    // Invalid/misspelled type names should return None
    assert_eq!(JuliaType::from_name("int64"), None); // Case sensitive
    assert_eq!(JuliaType::from_name("INTEGER"), None);
    assert_eq!(JuliaType::from_name("Func"), None); // Not "Function"
}

/// Issue #5157: register the base.jl numeric struct hierarchy that `CoreType`
/// subtyping derives from the explicit `StructHierarchy` (instead of hardcoded
/// type names).
fn numeric_struct_hierarchy() -> crate::types::StructHierarchy {
    let mut hierarchy = crate::types::StructHierarchy::new();
    hierarchy.insert("Complex", Some("Number".to_string()), Vec::new());
    hierarchy.insert("Rational", Some("Real".to_string()), Vec::new());
    hierarchy
}

#[test]
fn test_struct_subtype_of_number() {
    let hierarchy = numeric_struct_hierarchy();
    let number = crate::inference_core::CoreType::from(&JuliaType::Number);
    let struct_ty =
        |name: &str| crate::inference_core::CoreType::from(&JuliaType::Struct(name.to_string()));

    // Complex{T} <: Number for any T
    assert!(struct_ty("Complex{Float64}").is_subtype_of_with_hierarchy(&number, &hierarchy));
    assert!(struct_ty("Complex{Int64}").is_subtype_of_with_hierarchy(&number, &hierarchy));
    assert!(struct_ty("Complex{Bool}").is_subtype_of_with_hierarchy(&number, &hierarchy));
    assert!(struct_ty("Complex{Float32}").is_subtype_of_with_hierarchy(&number, &hierarchy));
    // Bare "Complex" (no type param) is also <: Number
    assert!(struct_ty("Complex").is_subtype_of_with_hierarchy(&number, &hierarchy));

    // Rational{T} <: Number
    assert!(struct_ty("Rational{Int64}").is_subtype_of_with_hierarchy(&number, &hierarchy));
    assert!(struct_ty("Rational").is_subtype_of_with_hierarchy(&number, &hierarchy));

    // Arbitrary user structs are NOT <: Number
    assert!(!struct_ty("Point{Float64}").is_subtype_of_with_hierarchy(&number, &hierarchy));
    assert!(!struct_ty("MyStruct").is_subtype_of_with_hierarchy(&number, &hierarchy));
}

#[test]
fn test_struct_subtype_of_real() {
    let hierarchy = numeric_struct_hierarchy();
    let real = crate::inference_core::CoreType::from(&JuliaType::Real);
    let struct_ty =
        |name: &str| crate::inference_core::CoreType::from(&JuliaType::Struct(name.to_string()));

    // Rational{T} <: Real
    assert!(struct_ty("Rational{Int64}").is_subtype_of_with_hierarchy(&real, &hierarchy));
    assert!(struct_ty("Rational").is_subtype_of_with_hierarchy(&real, &hierarchy));

    // Complex is NOT <: Real
    assert!(!struct_ty("Complex{Float64}").is_subtype_of_with_hierarchy(&real, &hierarchy));
    assert!(!struct_ty("Complex{Int64}").is_subtype_of_with_hierarchy(&real, &hierarchy));
    assert!(!struct_ty("Complex").is_subtype_of_with_hierarchy(&real, &hierarchy));

    // Arbitrary user structs are NOT <: Real
    assert!(!struct_ty("Point{Float64}").is_subtype_of_with_hierarchy(&real, &hierarchy));
}

// =============================================================================
// Diagonal Rule tests (Issue #2554)
// =============================================================================

#[test]
fn test_diagonal_rule_tuple_concrete_type() {
    use std::collections::HashMap;
    // Tuple{T, T} where T — T appears twice in covariant position
    // Binding T=Int64 (concrete) should pass
    let mut bindings = HashMap::new();
    bindings.insert("T".to_string(), JuliaType::Int64);
    let param_types = vec![
        JuliaType::Struct("T".to_string()),
        JuliaType::Struct("T".to_string()),
    ];
    assert!(JuliaType::check_diagonal_rule_for_params(
        &param_types,
        &bindings
    ));
}

#[test]
fn test_diagonal_rule_tuple_abstract_type() {
    use std::collections::HashMap;
    // Tuple{T, T} where T — T appears twice in covariant position
    // Binding T=Any (abstract) should FAIL
    let mut bindings = HashMap::new();
    bindings.insert("T".to_string(), JuliaType::Any);
    let param_types = vec![
        JuliaType::Struct("T".to_string()),
        JuliaType::Struct("T".to_string()),
    ];
    assert!(!JuliaType::check_diagonal_rule_for_params(
        &param_types,
        &bindings
    ));
}

#[test]
fn test_diagonal_rule_single_occurrence_ok() {
    use std::collections::HashMap;
    // Tuple{T, S} where {T, S} — each appears once, diagonal rule does NOT apply
    // Binding T=Any (abstract) should pass (no diagonal restriction)
    let mut bindings = HashMap::new();
    bindings.insert("T".to_string(), JuliaType::Any);
    bindings.insert("S".to_string(), JuliaType::Number);
    let param_types = vec![
        JuliaType::Struct("T".to_string()),
        JuliaType::Struct("S".to_string()),
    ];
    assert!(JuliaType::check_diagonal_rule_for_params(
        &param_types,
        &bindings
    ));
}

#[test]
fn test_diagonal_rule_invariant_position_ok() {
    use std::collections::HashMap;
    // Vector{T} where T — T appears once in invariant position
    // Even with abstract type, diagonal rule does NOT apply
    let mut bindings = HashMap::new();
    bindings.insert("T".to_string(), JuliaType::Number);
    let param_types = vec![JuliaType::VectorOf(Box::new(JuliaType::Struct(
        "T".to_string(),
    )))];
    assert!(JuliaType::check_diagonal_rule_for_params(
        &param_types,
        &bindings
    ));
}

#[test]
fn test_diagonal_rule_type_of_invariant() {
    use std::collections::HashMap;
    // Type{T}, Type{T} — T appears twice but in invariant position (inside TypeOf)
    // Diagonal rule does NOT apply because occurs_inv > 0
    let mut bindings = HashMap::new();
    bindings.insert("T".to_string(), JuliaType::Any);
    let param_types = vec![
        JuliaType::TypeOf(Box::new(JuliaType::Struct("T".to_string()))),
        JuliaType::TypeOf(Box::new(JuliaType::Struct("T".to_string()))),
    ];
    assert!(JuliaType::check_diagonal_rule_for_params(
        &param_types,
        &bindings
    ));
}

#[test]
fn test_diagonal_rule_struct_type_concrete() {
    use std::collections::HashMap;
    // f(x::T, y::T) where T — T appears twice in covariant position
    // Binding T=Struct("Point") (concrete) should pass
    let mut bindings = HashMap::new();
    bindings.insert("T".to_string(), JuliaType::Struct("Point".to_string()));
    let param_types = vec![
        JuliaType::Struct("T".to_string()),
        JuliaType::Struct("T".to_string()),
    ];
    assert!(JuliaType::check_diagonal_rule_for_params(
        &param_types,
        &bindings
    ));
}
