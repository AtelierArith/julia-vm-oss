# Test that math intrinsics preserve Float16/Float32 types (Issue #2221)
# In Julia, sqrt(Float32(4.0)) returns Float32, not Float64.

using Test

@testset "Math intrinsics type preservation" begin
    # Float32 preservation
    @test typeof(sqrt(Float32(4.0))) == Float32
    @test typeof(floor(Float32(3.7))) == Float32
    @test typeof(ceil(Float32(3.2))) == Float32
    @test typeof(trunc(Float32(3.9))) == Float32
    @test typeof(abs(Float32(-2.5))) == Float32
    @test typeof(abs2(Float32(-2.5))) == Float32
    @test typeof(sign(Float32(-2.5))) == Float32
    @test typeof(signbit(Float32(-2.5))) == Bool
    @test typeof(sin(Float32(0.5))) == Float32
    @test typeof(cos(Float32(0.5))) == Float32
    @test typeof(exp(Float32(1.0))) == Float32
    @test typeof(log(Float32(4.0))) == Float32

    # Float32 value correctness
    @test sqrt(Float32(4.0)) == Float32(2.0)
    @test floor(Float32(3.7)) == Float32(3.0)
    @test ceil(Float32(3.2)) == Float32(4.0)
    @test trunc(Float32(3.9)) == Float32(3.0)
    @test abs(Float32(-2.5)) == Float32(2.5)
    @test abs2(Float32(-2.5)) == Float32(6.25)
    @test sign(Float32(-2.5)) == Float32(-1.0)

    # Float16 preservation
    @test typeof(sqrt(Float16(4.0))) == Float16
    @test typeof(floor(Float16(3.5))) == Float16
    @test typeof(ceil(Float16(3.5))) == Float16
    @test typeof(trunc(Float16(3.5))) == Float16
    @test typeof(abs(Float16(-2.5))) == Float16
    @test typeof(abs2(Float16(-2.5))) == Float16
    @test typeof(sign(Float16(-2.5))) == Float16
    @test typeof(signbit(Float16(-2.5))) == Bool
    @test typeof(sin(Float16(0.5))) == Float16
    @test typeof(cos(Float16(0.5))) == Float16
    @test typeof(exp(Float16(1.0))) == Float16
    @test typeof(log(Float16(4.0))) == Float16

    # Integer and Bool sign/abs2 width preservation
    @test typeof(sign(Int8(-3))) == Int8
    @test typeof(sign(UInt8(3))) == UInt8
    @test typeof(sign(Int128(-3))) == Int128
    @test typeof(sign(true)) == Bool
    @test typeof(abs2(Int8(-3))) == Int8
    @test typeof(abs2(UInt8(3))) == UInt8
    @test typeof(signbit(Int8(-3))) == Bool
    @test typeof(signbit(UInt8(3))) == Bool
    @test typeof(signbit(true)) == Bool

    # Float64 still works
    @test typeof(sqrt(4.0)) == Float64
    @test typeof(floor(3.7)) == Float64
    @test typeof(ceil(3.2)) == Float64
    @test typeof(trunc(3.9)) == Float64
    @test typeof(abs(-2.5)) == Float64
    @test typeof(abs2(-2.5)) == Float64
    @test typeof(sign(-2.5)) == Float64
    @test typeof(signbit(-2.5)) == Bool
    @test typeof(sin(0.5)) == Float64
    @test typeof(cos(0.5)) == Float64
    @test typeof(exp(1.0)) == Float64
    @test typeof(log(4.0)) == Float64
    @test repr(sign(-0.0)) == "-0.0"
end

true
