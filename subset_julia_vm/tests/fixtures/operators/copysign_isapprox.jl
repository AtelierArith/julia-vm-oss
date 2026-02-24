# Test copysign and isapprox functions (Issue #1883)

using Test

@testset "copysign positive to positive" begin
    @test copysign(3.0, 1.0) == 3.0
end

@testset "copysign positive to negative" begin
    @test copysign(3.0, -1.0) == -3.0
end

@testset "copysign negative to positive" begin
    @test copysign(-3.0, 1.0) == 3.0
end

@testset "copysign negative to negative" begin
    @test copysign(-3.0, -1.0) == -3.0
end

@testset "copysign zero" begin
    @test copysign(0.0, 1.0) == 0.0
    @test copysign(5.0, 0.0) == 5.0
end

@testset "isapprox equal" begin
    @test isapprox(1.0, 1.0) == true
    @test isapprox(0.0, 0.0) == true
end

@testset "isapprox close" begin
    @test isapprox(1.0, 1.0 + 1e-10) == true
    @test isapprox(100.0, 100.0 + 1e-8) == true
end

@testset "isapprox not close" begin
    @test isapprox(1.0, 2.0) == false
    @test isapprox(0.0, 1.0) == false
end

@testset "isapprox custom tolerance" begin
    @test isapprox(1.0, 1.1, 0.0, 0.2) == true
    @test isapprox(1.0, 1.1, 0.0, 0.05) == false
end

true
