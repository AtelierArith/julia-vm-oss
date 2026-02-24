# Test @showtime macro - timing with expression display

using Test

@testset "@showtime macro" begin
    # Test that @showtime returns the correct value
    result = @showtime begin
        x = 0
        for i in 1:100
            x = x + i
        end
        x
    end
    @test result == 5050  # Sum of 1..100

    # Test simple expression
    val = @showtime 1 + 2
    @test val == 3
end

true  # Test passed
