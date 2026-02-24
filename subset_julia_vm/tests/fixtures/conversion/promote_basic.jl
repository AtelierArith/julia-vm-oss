# Test promote(x, y, ...) - type promotion

using Test

@testset "promote(x, y, ...) - type promotion to common type" begin
    t = promote(1, 2.0, 3)  # All become Float64
    # Note: Using parentheses to work around Issue #1053 (chained operator bug)
    @test ((t[1] + t[2]) + t[3]) == 6.0
end

true  # Test passed
