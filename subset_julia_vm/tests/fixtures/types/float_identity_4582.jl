using Test

@testset "Float identity and isequal parity (#4582 #4583 #4584)" begin
    @test Float32(1.5) === Float32(1.5)
    @test Float32(NaN) === Float32(NaN)
    @test !(Float32(-0.0) === Float32(0.0))
    @test isequal(Float32(NaN), Float32(NaN))
    @test !isequal(Float32(-0.0), Float32(0.0))
    @test !isequal(Float32(-0.0), 0.0)
    @test !isequal(0.0, Float32(-0.0))
    @test isequal(Float32(-0.0), -0.0)
    @test !isequal(Float32(-0.0), 0)
    @test !isequal(0, Float32(-0.0))

    @test Float16(1.5) === Float16(1.5)
    @test Float16(NaN) === Float16(NaN)
    @test !(Float16(-0.0) === Float16(0.0))
    @test isequal(Float16(NaN), Float16(NaN))
    @test !isequal(Float16(-0.0), Float16(0.0))
    @test !isequal(Float16(-0.0), 0.0)
    @test !isequal(0.0, Float16(-0.0))
    @test isequal(Float16(-0.0), -0.0)
    @test !isequal(Float16(-0.0), 0)
    @test !isequal(0, Float16(-0.0))

    @test true === true
    @test false === false
    @test !(true === false)
end

true
