using Test

@testset "Complex equality, isequal, hash" begin
    # === (object identity / egal)
    @test Complex(1.0, 2.0) === Complex(1.0, 2.0)
    @test !(Complex(1.0, 2.0) === Complex(1.0, 3.0))
    @test !(Complex(1.0, 2.0) === Complex(2.0, 2.0))

    # isequal
    @test isequal(Complex(1.0, 2.0), Complex(1.0, 2.0))
    @test !isequal(Complex(1.0, 2.0), Complex(1.0, 3.0))

    # hash consistency: equal values must have equal hashes
    @test hash(Complex(1.0, 2.0)) == hash(Complex(1.0, 2.0))
    @test hash(Complex(3.0, 4.0)) == hash(Complex(3.0, 4.0))
end

true
