# Issue #4818 (sibling to #4811/#4816): Vector{Any}(::Vector{S})
# returned the source vector unchanged instead of materializing a
# Vector{Any}. Same compile-time intercept in
# `compile_array_constructor` as #4811/#4816, but for T == Any.
#
# Fix: the prior typed-comprehension synthesis (#4815/#4817) cannot be
# reused for T == Any because `Any[x for x in arr]` lowers to a body
# wrapped in `Any(x)`, which is not a defined Julia constructor (and
# raises "Unknown function: Any" — tracked as #4819). Instead the
# intercept routes through a Pure-Julia helper
# `_vector_any_collect(arr)` that allocates `Vector{Any}(undef, n)`
# and copies each element via plain indexed store, which boxes each
# value to Any as a side effect of the Vector{Any} backing store.

using Test

@testset "Vector{Any}(::Vector{Int}) — boxes to Any (Issue #4818)" begin
    v = Vector{Any}([1, 2, 3])
    @test typeof(v) === Vector{Any}
    @test eltype(v) === Any
    @test length(v) == 3
    @test v[1] == 1
    @test v[2] == 2
    @test v[3] == 3
end

@testset "Vector{Any}(::Vector{Float64}) — boxes to Any (Issue #4818)" begin
    v = Vector{Any}([1.0, 2.0, 3.0])
    @test typeof(v) === Vector{Any}
    @test eltype(v) === Any
    @test v[1] == 1.0
end

@testset "Vector{Any}(::Vector{Any}) — same eltype copy (Issue #4818)" begin
    src = Vector{Any}([1, 2.0, "three"])
    v = Vector{Any}(src)
    @test typeof(v) === Vector{Any}
    @test eltype(v) === Any
    @test length(v) == 3
end

@testset "Vector{Any}() empty regression (Issue #4818)" begin
    # Empty Vector{Any}() stays on the empty-array path,
    # not on the new helper-call branch.
    v = Vector{Any}()
    @test typeof(v) === Vector{Any}
    @test length(v) == 0
end

@testset "Vector{Any}(undef, n) regression (Issue #4818)" begin
    # The undef pattern stays on the existing args.len()==2 branch.
    v = Vector{Any}(undef, 3)
    @test typeof(v) === Vector{Any}
    @test length(v) == 3
end

@testset "Vector{Int64}(::Vector{Int64}) regression — fast path (Issue #4818)" begin
    # Non-Any same-eltype case must keep the no-op fast path, not
    # accidentally regress into the Any-helper branch.
    src = [10, 20, 30]
    v = Vector{Int64}(src)
    @test typeof(v) === Vector{Int64}
    @test v == [10, 20, 30]
end

true
