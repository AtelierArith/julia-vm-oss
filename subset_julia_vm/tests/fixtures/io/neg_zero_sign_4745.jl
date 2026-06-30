using Test

@testset "repr(-0.0) preserves negative-zero sign (Issue #4745)" begin
    @test repr(-0.0) == "-0.0"
    @test repr(0.0) == "0.0"
    @test string(-0.0) == "-0.0"
    @test "$(-0.0)" == "-0.0"
end

@testset "negative zero round-trips through IO (Issue #4745)" begin
    io = IOBuffer()
    print(io, -0.0)
    @test String(take!(io)) == "-0.0"

    io2 = IOBuffer()
    show(io2, -0.0)
    @test String(take!(io2)) == "-0.0"
end

@testset "signbit / iszero still agree (Issue #4745)" begin
    # Cross-check that the underlying value retains its negative sign
    # (the display fix doesn't change the value itself).
    @test signbit(-0.0)
    @test !signbit(0.0)
    @test iszero(-0.0)
    @test iszero(0.0)
    @test -0.0 == 0.0  # IEEE 754: -0.0 == 0.0
end

true
