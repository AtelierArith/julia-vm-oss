# Test: for loop over String using iterate protocol

using Test

@testset "for loop over String using iterate protocol" begin
    s = "ABC"
    count = 0
    for c in s
        count += 1
    end
    @test (count) == 3.0
end

true  # Test passed
