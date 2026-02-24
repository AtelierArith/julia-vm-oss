# Test @timev macro - verbose timing output

using Test

@testset "@timev macro" begin
    # Test basic @timev usage (single argument form)
    result = @timev begin
        x = 0
        for i in 1:100
            x = x + i
        end
        x
    end
    @test result == 5050  # Sum of 1..100
end

true  # Test passed
