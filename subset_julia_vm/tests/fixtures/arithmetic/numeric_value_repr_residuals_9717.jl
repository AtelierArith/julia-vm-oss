using Test

@testset "Bool plus primitive negative zero preserves sign (Issue #9717)" begin
    @test typeof(false + -zero(Float64)) === Float64
    @test repr(false + -zero(Float64)) == "-0.0"
    @test repr(-zero(Float64) + false) == "-0.0"

    @test typeof(false + Float32(-0.0)) === Float32
    @test repr(false + Float32(-0.0)) == "-0.0f0"
    @test repr(Float32(-0.0) + false) == "-0.0f0"

    @test typeof(false + Float16(-0.0)) === Float16
    @test repr(false + Float16(-0.0)) == "Float16(-0.0)"
    @test repr(Float16(-0.0) + false) == "Float16(-0.0)"

    @test repr(true + -zero(Float64)) == "1.0"
    @test repr(true + Float32(-0.0)) == "1.0f0"
    @test repr(true + Float16(-0.0)) == "Float16(1.0)"
end

@testset "Float64 div by zero returns NaN while narrow floats keep Inf" begin
    @test isnan(div(true, 0.0))
    @test isnan(div(1, 0.0))
    @test isnan(div(1.0, 0.0))
    @test typeof(div(true, 0.0)) === Float64

    @test repr(div(Float32(1), Float32(0))) == "Inf32"
    @test repr(div(Float16(1), Float16(0))) == "Inf16"
end

@testset "Float32 exponent repr uses typed f marker (Issue #9717)" begin
    @test repr(Float32(1.5f8)) == "1.5f8"
    @test repr(Float32(1.5f-5)) == "1.5f-5"
    @test repr(Float32(2.1474836f9)) == "2.1474836f9"
    @test repr(Float32(1.7014118f38)) == "1.7014118f38"
end

@testset "Float16 mixed large integers promote before arithmetic (Issue #9717)" begin
    a = typemax(Int128)
    b = typemin(Int128)
    u = typemax(UInt16)
    y = Float16(2.5)

    @test repr(Float16(-0.0) * a) == "NaN16"
    @test repr(Float16(0.0) * b) == "NaN16"
    @test repr(Float16(Inf) + b) == "NaN16"
    @test repr(Float16(-Inf) - b) == "NaN16"
    @test repr(Float16(Inf) / a) == "NaN16"
    @test repr(u / y) == "Inf16"
    @test repr(rem(typemin(Int64), Float16(-1.0))) == "NaN16"
    @test repr(rem(u, y)) == "NaN16"
    @test repr(mod(u, y)) == "NaN16"
end

true
