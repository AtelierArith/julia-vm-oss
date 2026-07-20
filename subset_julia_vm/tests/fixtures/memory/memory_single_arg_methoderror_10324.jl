# Upstream `Memory{T}` has no single-argument constructor: the sized form is
# spelled `Memory{T}(undef, n)`. Both `Memory{T}(n::Int)` and `Memory{T}(undef)`
# are MethodErrors upstream — sjulia previously accepted `Memory{T}(n)` as an
# extension (Issue #10324 item 3). The zero-argument `Memory{T}()` and the
# `Memory{T}(undef, n)` forms remain valid.

using Test

@testset "Memory{T} single-argument constructor is a MethodError" begin
    @test_throws MethodError Memory{Int64}(4)
    @test_throws MethodError Memory{Float64}(3)
    @test_throws MethodError Memory{Int64}(undef)
end

@testset "Memory{T} upstream constructor forms still work" begin
    m = Memory{Int64}(undef, 4)
    @test length(m) == 4
    @test eltype(m) == Int64

    e = Memory{Int64}()
    @test length(e) == 0
    @test eltype(e) == Int64

    mf = Memory{Float64}(undef, 3)
    @test length(mf) == 3
    @test eltype(mf) == Float64
end

true
