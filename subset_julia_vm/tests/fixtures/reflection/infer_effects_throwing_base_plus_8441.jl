using Test

struct ThrowingPlus8441
    x::Int64
end

function Base.:+(a::ThrowingPlus8441, b::ThrowingPlus8441)
    error("boom")
end

plus_direct_8441(a, b) = +(a, b)
plus_syntax_8441(a, b) = a + b

@testset "infer_effects detects throwing user-defined Base.:+ (Issue #8441)" begin
    direct = Base.infer_effects(+, Tuple{ThrowingPlus8441,ThrowingPlus8441})
    wrapped_direct =
        Base.infer_effects(plus_direct_8441, Tuple{ThrowingPlus8441,ThrowingPlus8441})
    wrapped_syntax =
        Base.infer_effects(plus_syntax_8441, Tuple{ThrowingPlus8441,ThrowingPlus8441})

    @test direct.nothrow === false
    @test wrapped_direct.nothrow === false
    @test wrapped_syntax.nothrow === false
end

true
