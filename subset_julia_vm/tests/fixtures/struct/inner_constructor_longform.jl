# Long-form inner constructor with function ... end syntax

using Test

struct ValidatedPoint
    x::Float64
    y::Float64
    function ValidatedPoint(x, y)
        new(abs(x), abs(y))
    end
end

@testset "Long-form inner constructor with function...end syntax" begin

    p = ValidatedPoint(-5.0, -8.0)
    @test (p.x + p.y) == 13.0
end

true  # Test passed
