using Test

# Short-form function definitions with kwargs varargs (Issue #2242)
# The parser emits SplatExpression instead of SplatParameter for short-form

# Basic short-form with kwargs splat
f(x; kwargs...) = x + length(kwargs)

# Short-form with named kwarg + kwargs splat
g(x; y=0, kwargs...) = x + y + length(kwargs)

@testset "Short-form kwargs varargs (Issue #2242)" begin
    # Basic kwargs splat - must pass at least one kwarg (Issue #2247: empty kwargs bug)
    @test f(42; a=1, b=2) == 44
    @test f(10; a=1) == 11

    # Named kwarg + kwargs splat
    @test g(10; y=5, a=1) == 16
    @test g(10; y=5, a=1, b=2) == 17
end

true
