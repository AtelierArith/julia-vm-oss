# Complex number operations with im literals

using Test

@testset "Complex im literal operations" begin
    # Test complex creation
    z = 1 + 2im
    @test z isa Complex{Int64}
    @test z.re == 1
    @test z.im == 2

    z2 = 3 + 4im
    @test z2 isa Complex{Int64}
    @test z2.re == 3
    @test z2.im == 4

    # Test complex addition - THIS IS THE BUG
    sum = z + z2
    @test sum isa Complex{Int64}
    @test sum.re == 4
    @test sum.im == 6
end

true
