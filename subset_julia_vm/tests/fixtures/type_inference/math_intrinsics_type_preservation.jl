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

    # Float32 value correctness
    @test sqrt(Float32(4.0)) == Float32(2.0)
    @test floor(Float32(3.7)) == Float32(3.0)
    @test ceil(Float32(3.2)) == Float32(4.0)
    @test trunc(Float32(3.9)) == Float32(3.0)
    @test abs(Float32(-2.5)) == Float32(2.5)

    # Float16 preservation
    @test typeof(sqrt(Float16(4.0))) == Float16
    @test typeof(floor(Float16(3.5))) == Float16
    @test typeof(ceil(Float16(3.5))) == Float16
    @test typeof(trunc(Float16(3.5))) == Float16
    @test typeof(abs(Float16(-2.5))) == Float16

    # Float64 still works
    @test typeof(sqrt(4.0)) == Float64
    @test typeof(floor(3.7)) == Float64
    @test typeof(ceil(3.2)) == Float64
    @test typeof(trunc(3.9)) == Float64
    @test typeof(abs(-2.5)) == Float64
end

true
