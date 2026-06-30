# Issue #4802: Float64 print used Rust's default Display (always
# fixed-point), so very small / very large magnitudes produced
# multi-hundred-digit output. Upstream Julia uses scientific
# notation for `|x| < 1e-4` or `|x| >= 1e6`.
#
# Fix: format_float_julia routes through format_float_scientific_julia
# (which uses Rust's {:e} and adds a `.0` suffix for whole-mantissa
# cases) when magnitude is out of [1e-4, 1e6).

using Test

@testset "Float64 scientific notation — very small (Issue #4802)" begin
    @test string(1.5e-10) == "1.5e-10"
    @test string(1.0e-5) == "1.0e-5"
    @test string(2.5e-100) == "2.5e-100"
end

@testset "Float64 scientific notation — very large (Issue #4802)" begin
    @test string(1.5e20) == "1.5e20"
    @test string(1.0e308) == "1.0e308"
    @test string(2.5e15) == "2.5e15"
    @test string(1.5e6) == "1.5e6"
end

@testset "Float64 normal range stays fixed-point (Issue #4802)" begin
    @test string(0.0) == "0.0"
    @test string(0.5) == "0.5"
    @test string(3.14) == "3.14"
    @test string(2.0) == "2.0"
    @test string(150000.0) == "150000.0"   # 1.5e5 — still fixed
    @test string(100000.0) == "100000.0"
    @test string(0.0001) == "0.0001"        # 1e-4 — still fixed
end

@testset "Float64 scientific threshold (Issue #4802)" begin
    # 1e-5 → scientific; 0.0001 → fixed
    @test string(1.0e-4) == "0.0001"
    @test string(1.0e-5) == "1.0e-5"
    # 1e6 → scientific; 1e5 → fixed
    @test string(1.0e5) == "100000.0"
    @test string(1.0e6) == "1.0e6"
end

@testset "Float64 special values unchanged (Issue #4802)" begin
    @test string(NaN) == "NaN"
    @test string(Inf) == "Inf"
    @test string(-Inf) == "-Inf"
    @test string(-0.0) == "-0.0"
end

true
