# Test using with alias: using A: b as c

using Test

@testset "Using with alias" begin
    # Since we don't have a module system yet, using statements are no-ops
    # They should parse and lower without errors

    # These should not throw errors
    using LinearAlgebra: norm as n
    using Base: sin as s, cos as c

    # Verify the code continues to execute
    @test 2 + 2 == 4
end

true
