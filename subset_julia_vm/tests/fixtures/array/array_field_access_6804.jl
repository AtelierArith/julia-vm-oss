using Test

# Issue #6804: array field access (`a.size`, `a.ref`) — Array{T,N} is a Pure
# Julia mutable struct with fields `ref::MemoryRef{T}` and `size::NTuple{N,Int}`,
# so these must resolve to the struct fields. Top-level access already worked
# once arrays became the faithful Array wrapper; the remaining failure was
# `a.size` reached through a function parameter, where the lazy specializer
# mis-typed the wrapper's parametric `size`/`ref` fields and wrongly coerced the
# tuple result. Array field access is now left to the interpreter.

@testset "array .size / .ref top level (Issue #6804)" begin
    a = [1, 2, 3]
    @test a.size == (3,)
    @test typeof(a.size) == Tuple{Int64}
    @test typeof(a.ref) == MemoryRef{Int64}

    m = [1 2; 3 4]
    @test m.size == (2, 2)
    @test typeof(m.size) == Tuple{Int64, Int64}
end

@testset "array .size / .ref through function parameter (Issue #6804)" begin
    f(x) = x.size
    @test f([1, 2, 3]) == (3,)
    @test f([10, 20, 30, 40]) == (4,)
    v = [1, 2, 3]
    @test f(v) == (3,)

    g(x) = x.ref
    @test typeof(g([1, 2, 3])) == MemoryRef{Int64}
    @test typeof(g([1.0, 2.0])) == MemoryRef{Float64}
end

@testset "array .size after operations (Issue #6804)" begin
    v = Int[]
    push!(v, 5)
    push!(v, 6)
    @test v.size == (2,)
    @test [1.0, 2.0, 3.0].size == (3,)
end

true
