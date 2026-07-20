# Core.apply_type accepts splatted parameters for runtime UnionAll bases.
# Issues #10191, #10554, #10555, #10558, #10570, #10613.

using Test

struct PairApplyType10191{A,B}
    a::A
    b::B
end

struct TriApplyType10191{A,B,C}
    a::A
    b::B
    c::C
end

struct ManyApplyType10191{A,B,C,D,E,F,G,H,I,J,K,L,M,N,O,P,Q}
end

struct BoundedApplyType10554{T<:Real}
end

struct DependentUpperApplyType10570{T,U<:T}
end

struct DependentLowerApplyType10570{T,U>:T}
end

struct RuntimeTypeVarPair10613{A,B}
end

struct RuntimeTypeVarTriple10613{A,B,C}
end

struct RuntimeUnionAllFreePair10613{A,B}
end

struct RuntimeUnionAllFreeTriple10613{A,B,C}
end

struct RuntimeUnionAllBoundPair10613{A,B}
end

abstract type AbstractApplyType10554{T<:Real} end
abstract type AbstractLowerApplyType10554{T>:Int64} end

apply_splat_10191(base, args) = Core.apply_type(base, args...)
apply_mixed_10191(base, first, rest) = Core.apply_type(base, first, rest...)
apply_syntax_10191(base, args) = base{args...}

@testset "Core.apply_type dynamic-base splats (Issue #10191)" begin
    wrapper = PairApplyType10191
    args = Any[Int64, Float64]
    @test Core.apply_type(wrapper, args...) === PairApplyType10191{Int64, Float64}

    tuple_args = (String, Bool)
    @test apply_splat_10191(wrapper, tuple_args) === PairApplyType10191{String, Bool}
    @test apply_syntax_10191(wrapper, tuple_args) === PairApplyType10191{String, Bool}

    partial = Core.apply_type(wrapper, Int64)
    @test apply_splat_10191(partial, (Float64,)) === PairApplyType10191{Int64, Float64}

    tri_wrapper = TriApplyType10191
    @test apply_mixed_10191(tri_wrapper, Int64, Any[Float64, String]) ===
          TriApplyType10191{Int64, Float64, String}

    array_wrapper = Array
    @test apply_splat_10191(array_wrapper, Any[Int64, 2]) === Matrix{Int64}

    tuple_wrapper = Tuple
    @test apply_splat_10191(tuple_wrapper, Any[Int64, String]) === Tuple{Int64, String}

    typevar_args = Any[Int64, wrapper.body.var]
    typevar_result = apply_splat_10191(wrapper, typevar_args)
    @test typeof(typevar_result) === DataType
    @test string(typevar_result) == "PairApplyType10191{Int64, B}"
    @test typevar_result.parameters[2] === wrapper.body.var

    @test apply_splat_10191(wrapper, Any[]) === wrapper

    base_only = (wrapper,)
    first_segment = (Int64,)
    second_segment = (String,)
    @test Core.apply_type(base_only..., first_segment..., second_segment...) ===
          PairApplyType10191{Int64, String}
    base_and_first = (wrapper, Int64)
    @test Core.apply_type(base_and_first..., second_segment...) ===
          PairApplyType10191{Int64, String}

    bounded_wrapper = BoundedApplyType10554
    @test_throws TypeError Core.apply_type(bounded_wrapper, String)
    @test_throws TypeError Core.apply_type(bounded_wrapper, (String,)...)
    @test_throws TypeError Core.apply_type(BoundedApplyType10554, String)
    @test_throws Exception Core.apply_type(wrapper, Int64, String, Bool)
    @test_throws Exception Core.apply_type(wrapper, (Int64, String, Bool)...)
    @test_throws Exception Core.apply_type(PairApplyType10191, Int64, String, Bool)
    concrete = PairApplyType10191{Int64, String}
    @test_throws TypeError Core.apply_type(concrete, Bool)
    @test_throws TypeError Core.apply_type(concrete, (Bool,)...)
    @test_throws TypeError apply_splat_10191(concrete, ())

    upper_partial = Core.apply_type(DependentUpperApplyType10570, Real)
    @test Core.apply_type(upper_partial, Int64) ===
          DependentUpperApplyType10570{Real, Int64}
    @test Core.apply_type(DependentUpperApplyType10570, Real, Int64) ===
          DependentUpperApplyType10570{Real, Int64}
    @test_throws TypeError Core.apply_type(upper_partial, String)
    @test_throws TypeError Core.apply_type(DependentLowerApplyType10570, Real, Int64)

    abstract_wrapper = AbstractApplyType10554
    @test Core.apply_type(abstract_wrapper, Int64) === AbstractApplyType10554{Int64}
    @test_throws TypeError Core.apply_type(abstract_wrapper, String)
    @test_throws Exception Core.apply_type(abstract_wrapper, Int64, Float64)
    @test AbstractApplyType10554{Int64}.name.wrapper == abstract_wrapper
    @test typejoin(AbstractApplyType10554{Int64}, AbstractApplyType10554{Float64}) ===
          abstract_wrapper
    abstract_concrete = AbstractApplyType10554{Int64}
    @test_throws TypeError Core.apply_type(abstract_concrete)
    @test_throws TypeError Core.apply_type(abstract_concrete, Float64)
    @test Core.apply_type(AbstractLowerApplyType10554, Real) ===
          AbstractLowerApplyType10554{Real}
    @test_throws TypeError Core.apply_type(AbstractLowerApplyType10554, Bool)

    # Canonical Array-vs-Vector alias display/identity for bounded parameters
    # remains tracked by #10558; these assertions pin the bound interval itself.
    anonymous_both = Core.apply_type(Vector, TypeVar(:_, Int64, Real))
    @test occursin("Int64<:_<:Real", string(anonymous_both))
    @test anonymous_both !== Vector{<:Int64}
    @test !(anonymous_both <: Vector{<:Int64})
    named_both = Core.apply_type(Vector, TypeVar(:T, Int64, Real))
    @test occursin("Int64<:T<:Real", string(named_both))
    @test named_both <: Vector{<:Real}

    # Equal-looking TypeVars are still distinct binders. Applying an external
    # `T<:Real` must not identify it with the wrapper's own `T<:Real` binder.
    external_same_bounds = TypeVar(:T, Union{}, Real)
    external_result = Core.apply_type(BoundedApplyType10554, external_same_bounds)
    @test external_result.parameters[1] === external_same_bounds
    @test external_result !== BoundedApplyType10554

    # Distinct runtime TypeVars with identical metadata retain distinct
    # identities through application and later reflection (Issue #10613).
    first_t = TypeVar(:T)
    second_t = TypeVar(:T)
    first_vector = Core.apply_type(Vector, first_t)
    second_vector = Core.apply_type(Vector, second_t)
    @test first_vector.parameters[1] === first_t
    @test second_vector.parameters[1] === second_t
    @test first_vector.parameters[1] !== second_vector.parameters[1]
    @test first_vector !== second_vector
    bounded_t = TypeVar(:T, Union{}, Real)
    bounded_vector = Core.apply_type(Vector, bounded_t)
    @test bounded_vector.parameters[1] === bounded_t
    @test bounded_vector !== first_vector

    # Bounds that are themselves runtime TypeVars preserve the referenced
    # object identity, including through same-name nested UnionAll binders.
    outer_t = TypeVar(:T)
    inner_t = TypeVar(:T, Union{}, outer_t)
    @test inner_t.ub === outer_t
    runtime_pair_body = Core.apply_type(RuntimeTypeVarPair10613, outer_t, inner_t)
    inner_wrapper = UnionAll(inner_t, runtime_pair_body)
    nested_wrapper = UnionAll(outer_t, inner_wrapper)
    @test nested_wrapper.var === outer_t
    @test nested_wrapper.body.var === inner_t
    @test nested_wrapper.body.var.ub === nested_wrapper.var
    @test Core.apply_type(nested_wrapper, Real, Int64) ===
          RuntimeTypeVarPair10613{Real, Int64}
    @test Core.apply_type(nested_wrapper, Number, Real) ===
          RuntimeTypeVarPair10613{Number, Real}

    # Generated alpha names must avoid every original binder name, including a
    # later binder already named like the natural suffix of an earlier one.
    alpha_a = TypeVar(:T)
    alpha_b = TypeVar(:T1, Union{}, alpha_a)
    alpha_c = TypeVar(:T, Union{}, alpha_b)
    alpha_body = Core.apply_type(RuntimeTypeVarTriple10613, alpha_a, alpha_b, alpha_c)
    alpha_wrapper = UnionAll(alpha_a, UnionAll(alpha_b, UnionAll(alpha_c, alpha_body)))
    @test string(alpha_wrapper) ==
          "RuntimeTypeVarTriple10613{T, T1, T2} where {T, T1<:T, T2<:T1}"
    @test RuntimeTypeVarTriple10613{Real, Integer, Int64} <: alpha_wrapper
    @test !(RuntimeTypeVarTriple10613{Real, Int64, Integer} <: alpha_wrapper)

    # Each dependent upper bound resolves to the earlier binder's actual value.
    chain_a = TypeVar(:A)
    chain_b = TypeVar(:B, Union{}, chain_a)
    chain_c = TypeVar(:C, Union{}, chain_b)
    chain_body = Core.apply_type(RuntimeTypeVarTriple10613, chain_a, chain_b, chain_c)
    chain_wrapper = UnionAll(chain_a, UnionAll(chain_b, UnionAll(chain_c, chain_body)))
    @test RuntimeTypeVarTriple10613{Real, Integer, Int64} <: chain_wrapper
    @test !(RuntimeTypeVarTriple10613{Real, Int64, Integer} <: chain_wrapper)

    # Alpha-equivalent bound variables do not erase distinct free TypeVar IDs.
    free_f1 = TypeVar(:F)
    free_f2 = TypeVar(:F)
    bound_t1 = TypeVar(:T)
    bound_t2 = TypeVar(:T)
    free_wrapper1 = UnionAll(
        bound_t1,
        Core.apply_type(RuntimeUnionAllFreePair10613, bound_t1, free_f1),
    )
    free_wrapper2 = UnionAll(
        bound_t2,
        Core.apply_type(RuntimeUnionAllFreePair10613, bound_t2, free_f2),
    )
    @test free_wrapper1 !== free_wrapper2
    @test !(free_wrapper1 == free_wrapper2)
    @test !(free_wrapper1 <: free_wrapper2)
    @test !(free_wrapper2 <: free_wrapper1)
    @test typejoin(free_wrapper1, free_wrapper2) === RuntimeUnionAllFreePair10613

    # The same free IDs in different parameter positions are different types.
    swap_f1 = TypeVar(:F)
    swap_f2 = TypeVar(:F)
    swap_t1 = TypeVar(:T)
    swap_t2 = TypeVar(:T)
    swap_wrapper1 = UnionAll(
        swap_t1,
        Core.apply_type(RuntimeUnionAllFreeTriple10613, swap_t1, swap_f1, swap_f2),
    )
    swap_wrapper2 = UnionAll(
        swap_t2,
        Core.apply_type(RuntimeUnionAllFreeTriple10613, swap_t2, swap_f2, swap_f1),
    )
    @test swap_wrapper1 !== swap_wrapper2
    @test !(swap_wrapper1 == swap_wrapper2)
    @test !(swap_wrapper1 <: swap_wrapper2)
    @test !(swap_wrapper2 <: swap_wrapper1)
    @test typejoin(swap_wrapper1, swap_wrapper2) === RuntimeUnionAllFreeTriple10613

    # Free IDs in binder bounds retain their exact structural positions.
    bound_f1 = TypeVar(:F)
    bound_f2 = TypeVar(:F)
    bound_a1 = TypeVar(:A, Union{}, bound_f1)
    bound_b1 = TypeVar(:B, Union{}, bound_f2)
    bound_a2 = TypeVar(:A, Union{}, bound_f2)
    bound_b2 = TypeVar(:B, Union{}, bound_f1)
    bound_body1 = Core.apply_type(RuntimeUnionAllBoundPair10613, bound_a1, bound_b1)
    bound_body2 = Core.apply_type(RuntimeUnionAllBoundPair10613, bound_a2, bound_b2)
    bound_wrapper1 = UnionAll(bound_a1, UnionAll(bound_b1, bound_body1))
    bound_wrapper2 = UnionAll(bound_a2, UnionAll(bound_b2, bound_body2))
    @test bound_wrapper1 !== bound_wrapper2
    @test !(bound_wrapper1 == bound_wrapper2)
    @test !(bound_wrapper1 <: bound_wrapper2)
    @test !(bound_wrapper2 <: bound_wrapper1)
    @test typejoin(bound_wrapper1, bound_wrapper2) === RuntimeUnionAllBoundPair10613

    # A named `_` RuntimeTypeVar is still an identity-bearing free object.
    underscore_a = TypeVar(:_)
    underscore_b = TypeVar(:_)
    underscore_vector_a = Core.apply_type(Vector, underscore_a)
    underscore_vector_b = Core.apply_type(Vector, underscore_b)
    @test underscore_vector_a !== underscore_vector_b
    @test !(underscore_vector_a == underscore_vector_b)
    @test !(underscore_vector_a <: underscore_vector_b)
    @test !(underscore_vector_b <: underscore_vector_a)

    # A free runtime TypeVar never aliases a nominal type with the same name.
    for nominal in (Int64, Real, String, Module)
        free_nominal = TypeVar(Symbol(string(nominal)))
        free_vector = Core.apply_type(Vector, free_nominal)
        nominal_vector = Core.apply_type(Vector, nominal)
        @test free_vector !== nominal_vector
        @test !(free_vector == nominal_vector)
        @test !(free_vector <: nominal_vector)
        @test !(nominal_vector <: free_vector)
    end

    many_wrapper = ManyApplyType10191
    many_args = Any[
        Int8, Int16, Int32, Int64, Int128, UInt8, UInt16, UInt32, UInt64,
        UInt128, Float16, Float32, Float64, Bool, Char, String, Symbol,
    ]
    @test apply_splat_10191(many_wrapper, many_args) ===
          ManyApplyType10191{
              Int8, Int16, Int32, Int64, Int128, UInt8, UInt16, UInt32, UInt64,
              UInt128, Float16, Float32, Float64, Bool, Char, String, Symbol,
          }
end

true
