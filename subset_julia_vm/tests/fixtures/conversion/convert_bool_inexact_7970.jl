# Issue #7970: `convert(Bool, x)` (and the `Bool[...]` typed array literal that
# routes through it) must only accept `x == 0` (-> false) or `x == 1` (-> true),
# raising `InexactError` for any other value — matching upstream Julia's
# `Bool(x::Real) = x==0 ? false : x==1 ? true : throw(InexactError(...))` and
# `convert(::Type{Bool}, x::Number) = Bool(x)`.
#
# Previously sjulia used a lenient truthiness test (`x != 0`), so `convert(Bool, 2)`
# returned `true` and `Bool[2]` produced `Bool[1]` instead of erroring.
using Test

@testset "Issue #7970: convert(Bool, x) range validation" begin
    # Valid 0/1 values convert as before (signed, unsigned, and float sources).
    @test convert(Bool, 0) === false
    @test convert(Bool, 1) === true
    @test convert(Bool, 0x00) === false
    @test convert(Bool, 0x01) === true
    @test convert(Bool, 0.0) === false
    @test convert(Bool, 1.0) === true
    @test convert(Bool, true) === true
    @test convert(Bool, false) === false

    # Out-of-range values raise InexactError instead of a lenient truthiness test.
    @test_throws InexactError convert(Bool, 2)
    @test_throws InexactError convert(Bool, -1)
    @test_throws InexactError convert(Bool, 0x02)
    @test_throws InexactError convert(Bool, 2.0)
    @test_throws InexactError convert(Bool, 1.5)

    # Typed `Bool[...]` array literals convert each element through the same path.
    @test Bool[1, 0, 1] == [true, false, true]
    @test Bool[1, 0, 1] isa Vector{Bool}
    @test Bool[true, false] == [true, false]
    @test_throws InexactError Bool[2]
    @test_throws InexactError Bool[1, 0, 3]
end

true
