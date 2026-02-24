# Octal integer literal test

using Test

@testset "Octal integer literals: 0o0, 0o7, 0o10, 0o777" begin
    @assert 0o0 == 0
    @assert 0o7 == 7
    @assert 0o10 == 8
    @assert 0o17 == 15
    @assert 0o77 == 63
    @assert 0o777 == 511
    @assert 0o7_77 == 511
    @test (1.0) == 1.0
end

true  # Test passed
