# Test: Type narrowing with === nothing checks
using Test

# Function must be defined OUTSIDE @testset block per project guidelines
function safe_increment(x)
    if x === nothing
        return 0  # x is Nothing here
    else
        return x + 1  # x is non-Nothing here
    end
end

@testset "Nothing narrowing" begin
    @test safe_increment(nothing) == 0
    @test safe_increment(5) == 6
end

true
