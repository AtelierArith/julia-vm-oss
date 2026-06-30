using Test

struct PartialParamMatrix7734{A,B,T}
    data::Tuple
end

function PartialParamMatrix7734{A,B}(xs...) where {A,B}
    return PartialParamMatrix7734{A,B,typeof(xs[1])}((A, B, xs...))
end

struct FullyAppliedVararg7734{N,T}
    data::Tuple
end

function FullyAppliedVararg7734{N,T}(xs...) where {N,T}
    return FullyAppliedVararg7734{N,T}((N, T(xs[1]), xs[2], xs[3]))
end

m = PartialParamMatrix7734{2,2}(1, 2, 3, 4)
v = FullyAppliedVararg7734{3,Int64}(1, 2, 3)

ok = typeof(m) == PartialParamMatrix7734{2,2,Int64} &&
     m.data == (2, 2, 1, 2, 3, 4) &&
     typeof(v) == FullyAppliedVararg7734{3,Int64} &&
     v.data == (3, 1, 2, 3) &&
     typeof(v.data[2]) == Int64

@testset "partial parametric constructor calls (Issue #7734)" begin
    @test typeof(m) == PartialParamMatrix7734{2,2,Int64}
    @test m.data == (2, 2, 1, 2, 3, 4)
    @test typeof(v) == FullyAppliedVararg7734{3,Int64}
    @test v.data == (3, 1, 2, 3)
    @test typeof(v.data[2]) == Int64
end

ok
