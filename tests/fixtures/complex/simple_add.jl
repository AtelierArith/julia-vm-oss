# Simple test: Create two Complex{Int64} directly and add them

using Test

@testset "Simple Complex{Int64} addition" begin
    # Create Complex directly without im
    z1 = Complex{Int64}(1, 2)
    z2 = Complex{Int64}(3, 4)

    # This should work - both types are known to be Complex{Int64}
    sum = z1 + z2

    @test sum.re == 4
    @test sum.im == 6
end

true
