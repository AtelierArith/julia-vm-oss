# Test typeof(generator) returns Generator type (Issue #414)
# In Julia: typeof((x for x in 1:3)) == Base.Generator{...}
# We check that typeof returns the correct string representation

using Test

@testset "typeof(generator) returns Base.Generator (Issue #414)" begin
    g = (x^2 for x in 1:5)
    @test (string(typeof(g)) == "Base.Generator")
end

true  # Test passed
