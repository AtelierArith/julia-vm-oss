using Test

# Float16 arithmetic and comparisons
@testset "Float16 arithmetic" begin
    a = Float16(2.0)
    b = Float16(3.0)

    @test a + b == Float16(5.0)
    @test b - a == Float16(1.0)
    @test a * b == Float16(6.0)
    @test b / a == Float16(1.5)

    # Float16 type preservation
    @test typeof(a + b) == Float16
    @test typeof(a * b) == Float16

    # Comparison
    @test Float16(1.0) < Float16(2.0)
    @test Float16(3.0) > Float16(2.0)
    @test Float16(1.0) == Float16(1.0)
end

true
