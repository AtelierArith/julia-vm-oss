# Nested parametric type literals preserve free runtime TypeVar identity.

using Test

module PartialOwnerA10460
    struct Owned{T,N} end
    struct Dependent{T,N<:T} end
    const IntDependent = Dependent{Int}
end
module PartialOwnerB10460
    struct Owned{T,N} end
end

@testset "Nested runtime TypeVar identity (Issue #10861)" begin
    a = TypeVar(:A)
    vector_a = Vector{a}

    literal = Tuple{Vector{a}}
    applied = Core.apply_type(Tuple, vector_a)
    @test literal.parameters[1] === vector_a
    @test literal === applied
    @test literal == applied

    nested_literal = Tuple{Tuple{Vector{a}}}
    nested_applied = Core.apply_type(Tuple, Core.apply_type(Tuple, vector_a))
    @test nested_literal.parameters[1].parameters[1] === vector_a
    @test nested_literal === nested_applied

    dict_literal = Tuple{Dict{Symbol,Vector{a}}}
    dict_applied = Core.apply_type(
        Tuple,
        Core.apply_type(Dict, Symbol, vector_a),
    )
    @test dict_literal.parameters[1].parameters[2] === vector_a
    @test dict_literal === dict_applied

    singleton_literal = Type{vector_a}
    singleton_applied = Core.apply_type(Type, vector_a)
    @test singleton_literal === singleton_applied
    @test singleton_literal <: Type

    partial_array = Array{vector_a}
    @test partial_array isa UnionAll
    @test partial_array.body.parameters[1] === vector_a
    @test Core.apply_type(partial_array, 1) === Array{vector_a,1}

    struct RuntimeNestedPair{X,Y<:Real} end
    partial_user = RuntimeNestedPair{vector_a}
    @test partial_user isa UnionAll
    @test partial_user.var.ub === Real
    @test partial_user.body.parameters[1] === vector_a
    build_partial_user(x) = RuntimeNestedPair{x}
    dynamic_partial_user = build_partial_user(vector_a)
    @test dynamic_partial_user isa UnionAll
    @test dynamic_partial_user.var.ub === Real
    @test dynamic_partial_user.body.parameters[1] === vector_a

    struct RuntimeNestedDependent{X,Y<:X} end
    dependent_literal = RuntimeNestedDependent{vector_a}
    dependent_applied = Core.apply_type(RuntimeNestedDependent, vector_a)
    @test dependent_literal.var.ub === vector_a
    @test dependent_applied.var.ub === vector_a
    @test dependent_literal === dependent_applied

    struct ConcreteDependent10460{T,N<:T} end
    @test ConcreteDependent10460{Int}.var.ub === Int
    @test ConcreteDependent10460{Real}.var.ub === Real
    @test !(ConcreteDependent10460{Int}.var === ConcreteDependent10460{Real}.var)

    @test !(PartialOwnerA10460.Owned{Int}.var === PartialOwnerB10460.Owned{Int}.var)
    owner_t = TypeVar(:OwnerT10460)
    owner_a = PartialOwnerA10460.Owned{owner_t,Int}
    owner_b = PartialOwnerB10460.Owned{owner_t,Int}
    @test !(owner_a == owner_b)
    @test !(owner_a === owner_b)
    @test PartialOwnerA10460.IntDependent.var.ub === Int
    @test !(PartialOwnerA10460.IntDependent.var === PartialOwnerA10460.Dependent{Int}.var)
    qualified_free = TypeVar(:QualifiedFree10460)
    qualified_partial = PartialOwnerA10460.Dependent{qualified_free}
    @test qualified_partial.var.ub === qualified_free
    @test typeof(qualified_partial.var.ub) === TypeVar

    b = TypeVar(:B, Union{}, Vector{String})
    wrapper = UnionAll(a, UnionAll(b, Tuple{a,b}))
    reconstructed = Core.apply_type(wrapper, Int, Vector{a})
    explicit = Tuple{Int,Vector{a}}
    @test reconstructed.parameters[1] === explicit.parameters[1]
    @test reconstructed.parameters[2] === explicit.parameters[2]
    @test reconstructed == explicit
    @test reconstructed === explicit

    struct RuntimeDispatchWrap{T} end
    matches_runtime_wrap(::Type{RuntimeDispatchWrap{T}}) where T = true
    matches_runtime_wrap(::Type) = false
    existential = UnionAll(a, RuntimeDispatchWrap{Vector{a}})
    @test !matches_runtime_wrap(existential)

    # One where parameter must unify across identity-bearing structured images.
    repeated_pair(::Type{Pair{T,T}}) where T = true
    repeated_pair(::Type) = false
    same_name_distinct = TypeVar(:A)
    @test repeated_pair(Pair{a,a})
    @test !repeated_pair(Pair{a,same_name_distinct})

    struct RuntimeDispatchLowerBound{T} end
    lower_bounded(::Type{SubArray{T}}) where {T>:RuntimeDispatchLowerBound{Int}} = :bounded
    lower_bounded(::Type) = :fallback
    call_runtime_lower_bounded(f, t) = f(t)
    @test call_runtime_lower_bounded(lower_bounded, SubArray{Int8}) === :fallback

    partial_eltype = Vector{SubArray{Int8}}(undef, 0)
    @test eltype(partial_eltype) === SubArray{Int8}
    @test eltype(partial_eltype) <: SubArray{Int8}

    rank_three_partial = Array{SubArray{Int8},3}(undef, 0, 0, 0)
    @test eltype(rank_three_partial) === SubArray{Int8}
    @test typeof(rank_three_partial).parameters[1] === SubArray{Int8}

    struct PartialIdentity10460{T,N} end
    fresh_n = TypeVar(:N)
    fresh_partial = UnionAll(fresh_n, PartialIdentity10460{Int8,fresh_n})
    @test PartialIdentity10460{Int8} == fresh_partial
    @test PartialIdentity10460{Int8} === fresh_partial

    fresh_x = TypeVar(:X)
    alpha_partial = UnionAll(fresh_x, PartialIdentity10460{Int8,fresh_x})
    @test PartialIdentity10460{Int8} == alpha_partial
    @test !(PartialIdentity10460{Int8} === alpha_partial)

    bounded_n = TypeVar(:N, Union{}, Real)
    bounded_partial = UnionAll(bounded_n, PartialIdentity10460{Int8,bounded_n})
    @test !(PartialIdentity10460{Int8} === bounded_partial)

    outer_t = TypeVar(:T, Union{}, Real)
    free_t = TypeVar(:T, Union{}, String)
    inner_s = TypeVar(:S, Union{}, free_t)
    nested_bounds = UnionAll(outer_t, UnionAll(inner_s, Tuple{outer_t,inner_s,free_t}))
    @test nested_bounds.body.var.ub === free_t
    @test !(nested_bounds.body.var.ub === outer_t)

    struct RuntimeFieldHolder10460{T}
        value::T
    end
    field_n = TypeVar(:FieldN10460)
    field_unionall = UnionAll(field_n, Tuple{field_n})
    field_holder = RuntimeFieldHolder10460{field_unionall}
    @test fieldtype(field_holder, 1) === field_unionall
    @test fieldtypes(field_holder)[1] === field_unionall
end

true
