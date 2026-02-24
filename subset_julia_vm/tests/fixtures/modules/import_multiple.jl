# Test import with multiple items: import A: b, c

using Test

@testset "Import with multiple items" begin
    # Since we don't have a module system yet, import statements are no-ops
    # They should parse and lower without errors

    # These should not throw errors
    import Base: sin, cos
    import Base: abs, sqrt, exp

    # Verify the code continues to execute
    @test 2 + 2 == 4
end

true
