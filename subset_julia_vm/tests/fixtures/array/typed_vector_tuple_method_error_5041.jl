# Issue #5041: Vector{T}(::Tuple) / Array{T}(::Tuple) must raise MethodError.
#
# Upstream Julia has *no* `Array{T}(::Tuple)` / `Vector{T}(::Tuple)` constructor
# method — `Vector{Int}((1, 2, 3))` raises a `MethodError` (the correct spellings
# are `collect((1,2,3))` or `Int[(1,2,3)...]`). sjulia's single-arg
# `compile_array_constructor` intercept previously treated any iterable-ish
# argument as an array/range to materialize and silently built a vector from a
# tuple — an undocumented out-of-cluster divergence from the resolved
# #4811/#4816/#4818/#4819/#4822 set (those covered Range / Array / empty-array /
# Any shapes, never Tuple).
#
# The fix guards the Tuple argument shape and synthesizes the same catchable
# runtime `MethodError(ctor, (tuple,))` upstream raises (rendering, for the typed
# form, exactly `no method matching Vector{Int64}(::Tuple{...})`). The legitimate
# Range / Array / undef-dims / collect / comprehension paths are unaffected.
#
# Every assertion below was verified to match upstream Julia 1.12.

using Test

# ---- now-erroring cases: Tuple argument has no constructor method (#5041) ----
@testset "Vector{T}(::Tuple) raises MethodError (#5041)" begin
    @test_throws MethodError Vector{Int64}((1, 2, 3))
    @test_throws MethodError Vector{Float64}((1, 2, 3))
    @test_throws MethodError Vector{Any}((1, 2, 3))
    @test_throws MethodError Vector{Int64}((1,))
end

@testset "Array{T}/bare-alias (::Tuple) raises MethodError (#5041)" begin
    @test_throws MethodError Array{Int64,1}((1, 2, 3))
    @test_throws MethodError Vector((1, 2, 3))
    @test_throws MethodError Array((1, 2, 3))
end

# ---- regression guard: tuple-from-array constructions still work (#5041) ----
@testset "collect(tuple) and tuple comprehension still work (#5041)" begin
    @test collect((1, 2, 3)) == [1, 2, 3]
    @test typeof(collect((1, 2, 3))) === Vector{Int64}
    @test [x for x in (1, 2, 3)] == [1, 2, 3]
    @test [2x for x in (10, 20, 30)] == [20, 40, 60]
end

# ---- regression guard: valid Vector/Array constructors untouched (#5041) ----
@testset "valid Vector/Array constructors untouched (#5041)" begin
    # undef-sized allocation
    @test length(Vector{Int64}(undef, 3)) == 3
    # range argument (materialize + convert eltype)
    @test Vector{Float64}(1:3) == [1.0, 2.0, 3.0]
    @test typeof(Vector{Float64}(1:3)) === Vector{Float64}
    @test Vector{Int64}(1:3) == [1, 2, 3]
    # array argument (convert eltype / box to Any)
    @test Vector{Float64}([1, 2, 3]) == [1.0, 2.0, 3.0]
    @test typeof(Vector{Float64}([1, 2, 3])) === Vector{Float64}
    @test Vector{Any}([1, 2, 3]) == Any[1, 2, 3]
    @test typeof(Vector{Any}([1, 2, 3])) === Vector{Any}
    # tuple as DIMS arg to undef allocation is valid (tuple is dims, not data)
    @test size(Array{Int64}(undef, (2, 3))) == (2, 3)
end

true
