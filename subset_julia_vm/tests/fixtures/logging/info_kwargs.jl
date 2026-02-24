# Test @info macro with variable in message
# Using string interpolation to include values in the message.

using Test

@testset "@info with vars" begin
    x = 10
    y = 20

    # Variables in message via interpolation
    @info "Value of x is $x"
    @info "Values are x=$x and y=$y"

    @test true
end

true
