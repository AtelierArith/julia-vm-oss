# Test typeof(generator) returns Generator type (Issue #414)
# In Julia: typeof((x for x in 1:3)) == Base.Generator{...}
# We check that typeof returns a parametric Base.Generator shape.

using Test

@testset "typeof(generator) returns Base.Generator (Issue #414)" begin
    g = (x^2 for x in 1:5)
    type_name = string(typeof(g))
    @test startswith(type_name, "Base.Generator{")
    @test typeof(Base.IteratorSize(typeof(g))) === typeof(Base.HasShape{1}())
    @test typeof(Base.IteratorEltype(typeof(g))) === typeof(Base.EltypeUnknown())
end

true  # Test passed
