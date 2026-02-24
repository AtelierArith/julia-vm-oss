# Test deg2rad and rad2deg conversion functions

using Test

@testset "Angle conversion functions" begin
    # deg2rad: degrees to radians
    # 180 degrees = π radians
    @test abs(deg2rad(180) - π) < 1e-10
    @test abs(deg2rad(90) - π/2) < 1e-10
    @test abs(deg2rad(45) - π/4) < 1e-10
    @test abs(deg2rad(0) - 0.0) < 1e-10
    @test abs(deg2rad(360) - 2π) < 1e-10

    # Negative angles
    @test abs(deg2rad(-90) - (-π/2)) < 1e-10
    @test abs(deg2rad(-180) - (-π)) < 1e-10

    # rad2deg: radians to degrees
    # π radians = 180 degrees
    @test abs(rad2deg(π) - 180.0) < 1e-10
    @test abs(rad2deg(π/2) - 90.0) < 1e-10
    @test abs(rad2deg(π/4) - 45.0) < 1e-10
    @test abs(rad2deg(0.0) - 0.0) < 1e-10
    @test abs(rad2deg(2π) - 360.0) < 1e-10

    # Negative angles
    @test abs(rad2deg(-π/2) - (-90.0)) < 1e-10
    @test abs(rad2deg(-π) - (-180.0)) < 1e-10

    # Round-trip conversion
    @test abs(rad2deg(deg2rad(123.0)) - 123.0) < 1e-10
    @test abs(deg2rad(rad2deg(1.5)) - 1.5) < 1e-10
end

true
