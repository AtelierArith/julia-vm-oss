# Test widemul function
# Related to issue #487

using Test

@testset "widemul: multiply with widening (Issue #487)" begin

    # widemul multiplies and widens the result
    test1 = widemul(10, 20) == 200
    test2 = widemul(100, 100) == 10000

    # Float widening
    test3 = widemul(2.5, 4.0) == 10.0

    @test (test1 && test2 && test3)
end

true  # Test passed
