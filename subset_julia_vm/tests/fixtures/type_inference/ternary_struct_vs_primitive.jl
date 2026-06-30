# Ternary inference must not silently drop a non-struct branch when the other is a struct.
# Issue #3533

using Test

struct S3533
    x::Int64
end

function f3533(c)
    v = c ? S3533(1) : 0
    return v
end

function g3533(c)
    v = c ? S3533(2) : nothing
    return v
end

@testset "Ternary struct vs primitive branches" begin
    @test f3533(true) isa S3533
    @test f3533(true).x == 1
    @test f3533(false) == 0

    @test g3533(true) isa S3533
    @test g3533(false) === nothing
end

true
