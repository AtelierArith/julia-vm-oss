# Test that variable types can change dynamically (Julia semantics)
# x = 1 (Int64) -> x = 1.0 (Float64) -> x = 2 (Int64)
# Use isa() which is well supported

using Test

function test_dynamic_type()
    x = 1
    t1_ok = isa(x, Int64)  # true
    x = 1.0
    t2_ok = isa(x, Float64)  # true
    x = 2
    t3_ok = isa(x, Int64)  # Should be true, not false
    # Return true if all types are correct
    t1_ok && t2_ok && t3_ok
end

@testset "Variable types can change dynamically (x=1 -> x=1.0 -> x=2 preserves Int64)" begin

    @test (test_dynamic_type())
end

true  # Test passed
