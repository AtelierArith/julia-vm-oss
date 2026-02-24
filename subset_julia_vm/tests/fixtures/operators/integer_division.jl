# Test integer division operator ÷
# 7 ÷ 3 = floor(7/3) = 2

using Test

@testset "÷ integer division operator (floor division)" begin
    @test (7 ÷ 3) == 2.0
end

true  # Test passed
