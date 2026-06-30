using Test

# Issue #4974: representative tuple / pair / range helper signatures must report
# the upstream `Base.infer_effects` / `Base.infer_exception_type` records.
#
# Tuple `first`/`last`/`length`/`isempty`/`reverse`/`only` are total (no
# exception). `getindex(::Pair, ::Int)` is total-except-nothrow (may throw an
# out-of-range index). `eachindex(::AbstractVector)` is the refined
# `(?c,+e,+n,+t,+s,?m,+u,+o,+r)` (consistent-if-inaccessiblememonly /
# inaccessiblemem-or-argmemonly) and is nothrow. `collect(::AbstractRange)`
# allocates: `(!c,+e,!n,!t,+s,+m,!u,+o,+r)`, exception `Any` — upstream infers
# the same record for every concrete range type (`UnitRange`, `Base.OneTo`,
# `StepRange`), so `UnitRange{Int64}` is used here for a spelling that resolves
# identically under both `sjulia` and upstream `julia`. `tuple(...)` is a Core
# builtin already classified by the #4274 builtin layer (total). Values captured
# field-for-field from Julia 1.12.6.

@testset "infer_effects tuple helpers total (#4974)" begin
    for f in (first, last, length, isempty, reverse)
        @test string(Base.infer_effects(f, Tuple{Tuple{Int64,Int64}})) == "(+c,+e,+n,+t,+s,+m,+u,+o,+r)"
        @test Base.infer_exception_type(f, Tuple{Tuple{Int64,Int64}}) === Union{}
    end
    @test string(Base.infer_effects(only, Tuple{Tuple{Int64}})) == "(+c,+e,+n,+t,+s,+m,+u,+o,+r)"
    @test Base.infer_exception_type(only, Tuple{Tuple{Int64}}) === Union{}

    # tuple builtin (classified by the #4274 builtin-category layer) is total.
    @test string(Base.infer_effects(tuple, Tuple{Int64,Float64})) == "(+c,+e,+n,+t,+s,+m,+u,+o,+r)"
    @test Base.infer_exception_type(tuple, Tuple{Int64,Float64}) === Union{}
end

@testset "infer_effects getindex(::Pair, ::Int) total-except-nothrow (#4974)" begin
    @test string(Base.infer_effects(getindex, Tuple{Pair{Int64,Int64},Int64})) == "(+c,+e,!n,+t,+s,+m,+u,+o,+r)"
    @test Base.infer_exception_type(getindex, Tuple{Pair{Int64,Int64},Int64}) === Any
end

@testset "infer_effects eachindex / collect range helpers (#4974)" begin
    @test string(Base.infer_effects(eachindex, Tuple{Vector{Int64}})) == "(?c,+e,+n,+t,+s,?m,+u,+o,+r)"
    @test Base.infer_exception_type(eachindex, Tuple{Vector{Int64}}) === Union{}

    @test string(Base.infer_effects(collect, Tuple{UnitRange{Int64}})) == "(!c,+e,!n,!t,+s,+m,!u,+o,+r)"
    @test Base.infer_exception_type(collect, Tuple{UnitRange{Int64}}) === Any
end

@testset "infer_effects tuple/pair helpers do not intercept other overloads (#4974)" begin
    # getindex(::Pair, ::Int) classification must not affect getindex(::Vector,…).
    @test string(Base.infer_effects(getindex, Tuple{Vector{Int64},Int64})) != "(+c,+e,!n,+t,+s,+m,+u,+o,+r)" ||
          Base.infer_exception_type(getindex, Tuple{Vector{Int64},Int64}) === Union{}
    # collect(::AbstractRange) must not intercept collect(::Vector).
    @test string(Base.infer_effects(collect, Tuple{Vector{Int64}})) != "(!c,+e,!n,!t,+s,+m,!u,+o,+r)"
end

true
