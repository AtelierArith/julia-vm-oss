# Issue #3557: typed empty array literals `Int128[]` and `UInt128[]` should
# preserve their declared element type at runtime, even though the underlying
# storage is the boxed `Vec<Value>` path. The 64-bit-and-smaller types are
# already covered by Issue #3548.
using Test

@testset "Issue #3557 Int128/UInt128 typed empty arrays" begin
    a = Int128[]
    @test typeof(a) === Vector{Int128}
    @test eltype(a) === Int128
    @test isempty(a)

    b = UInt128[]
    @test typeof(b) === Vector{UInt128}
    @test eltype(b) === UInt128
    @test isempty(b)

    # Push! preserves element type
    push!(a, Int128(1))
    push!(a, Int128(2))
    @test typeof(a) === Vector{Int128}
    @test length(a) == 2
    @test a[1] === Int128(1)
    @test a[2] === Int128(2)

    push!(b, UInt128(1))
    push!(b, UInt128(2))
    @test typeof(b) === Vector{UInt128}
    @test length(b) == 2
    @test b[1] === UInt128(1)
    @test b[2] === UInt128(2)
end

true
