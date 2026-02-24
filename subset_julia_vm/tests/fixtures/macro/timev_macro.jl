# Test @timev macro - verbose timing measurement
# @timev prints elapsed time in seconds and nanoseconds

using Test

@testset "@timev macro" begin
    # @timev returns the value of the expression
    result = @timev sum(1:10)
    @test result == 55

    # @timev with a simple expression
    x = @timev 1 + 2 + 3
    @test x == 6

    # Test that the value is correctly returned
    y = @timev 2 * 21
    @test y == 42
end

true
