# Test typed Return/Store/Load instructions for Float32 and Float16
# Issue #1893: F32/F16 previously fell through to ReturnAny/StoreAny/LoadAny

using Test

# Float32 function return
function return_f32()
    Float32(3.14)
end

# Float16 function return
function return_f16()
    Float16(2.71)
end

# Float32 local variable store/load roundtrip
function f32_store_load()
    x = Float32(1.5)
    y = Float32(2.5)
    x + y
end

# Float32 roundtrip through function
function f32_identity(x)
    x
end

# Float16 roundtrip through function
function f16_identity(x)
    x
end

@testset "Typed F32/F16 instructions" begin
    @test typeof(return_f32()) == Float32
    @test return_f32() == Float32(3.14)

    @test typeof(return_f16()) == Float16
    @test return_f16() == Float16(2.71)

    @test typeof(f32_store_load()) == Float32
    @test f32_store_load() == Float32(4.0)

    @test typeof(f32_identity(Float32(5.0))) == Float32
    @test f32_identity(Float32(5.0)) == Float32(5.0)

    @test typeof(f16_identity(Float16(3.0))) == Float16
    @test f16_identity(Float16(3.0)) == Float16(3.0)
end

true
