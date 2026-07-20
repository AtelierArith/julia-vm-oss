# Issue #7971: the `Bool(x)` numeric constructor is callable. Issue #8768 moved
# the implementation back to Pure Julia instead of keeping a Rust builtin.
#
# It mirrors upstream `Bool(x::Real) = x==0 ? false : x==1 ? true :
# throw(InexactError(:Bool, Bool, x))`, so only 0/1 succeed.
using Test

@testset "Issue #7971: Bool(x) constructor" begin
    # Valid 0/1 values from signed, unsigned, float, and Bool sources.
    @test Bool(0) === false
    @test Bool(1) === true
    @test Bool(0x00) === false
    @test Bool(0x01) === true
    @test Bool(0.0) === false
    @test Bool(1.0) === true
    @test Bool(true) === true
    @test Bool(false) === false

    # Out-of-range values raise InexactError, like upstream.
    @test_throws InexactError Bool(2)
    @test_throws InexactError Bool(-1)
    @test_throws InexactError Bool(2.0)
    @test_throws InexactError Bool(0x02)

    # `Bool` is usable as a first-class function value (e.g. `map`).
    @test map(Bool, [0, 1, 0]) == [false, true, false]
    @test map(Bool, [0, 1]) isa Vector{Bool}

    # `Bool` still works as a type in type positions (no regression).
    @test Bool[true, false] == [true, false]
    @test Vector{Bool}(undef, 2) isa Vector{Bool}
    @test true isa Bool
    @test eltype(zeros(Bool, 2)) === Bool
end

true
