# Comprehensive typed array IndexStore test for all numeric element types (Issue #2218)
# Verifies that Vector{T}(undef, n) followed by indexed assignment works for every
# supported numeric type. Prevents regressions where new types are added but
# IndexStore/IndexLoad handlers are not updated.

using Test

@testset "Typed array IndexStore: Int64 and Float64" begin
    # Int64
    v = Vector{Int64}(undef, 2)
    v[1] = Int64(1)
    v[2] = Int64(-1)
    @test v[1] == Int64(1)
    @test v[2] == Int64(-1)
    @test typeof(v) == Vector{Int64}

    # Float64
    v = Vector{Float64}(undef, 2)
    v[1] = 3.14
    v[2] = -1.0
    @test v[1] == 3.14
    @test v[2] == -1.0
    @test typeof(v) == Vector{Float64}
end

@testset "Typed array IndexStore: small integer types" begin
    # Int8
    v = Vector{Int8}(undef, 2)
    v[1] = Int8(42)
    v[2] = Int8(-1)
    @test v[1] == Int8(42)
    @test v[2] == Int8(-1)
    @test typeof(v) == Vector{Int8}

    # Int16
    v = Vector{Int16}(undef, 2)
    v[1] = Int16(1000)
    v[2] = Int16(-500)
    @test v[1] == Int16(1000)
    @test v[2] == Int16(-500)
    @test typeof(v) == Vector{Int16}

    # Int32
    v = Vector{Int32}(undef, 2)
    v[1] = Int32(100000)
    v[2] = Int32(-50000)
    @test v[1] == Int32(100000)
    @test v[2] == Int32(-50000)
    @test typeof(v) == Vector{Int32}
end

@testset "Typed array IndexStore: unsigned integer types" begin
    # UInt8
    v = Vector{UInt8}(undef, 2)
    v[1] = UInt8(255)
    v[2] = UInt8(0)
    @test v[1] == UInt8(255)
    @test v[2] == UInt8(0)
    @test typeof(v) == Vector{UInt8}

    # UInt16
    v = Vector{UInt16}(undef, 2)
    v[1] = UInt16(65535)
    v[2] = UInt16(0)
    @test v[1] == UInt16(65535)
    @test v[2] == UInt16(0)
    @test typeof(v) == Vector{UInt16}

    # UInt32
    v = Vector{UInt32}(undef, 2)
    v[1] = UInt32(100000)
    v[2] = UInt32(0)
    @test v[1] == UInt32(100000)
    @test v[2] == UInt32(0)
    @test typeof(v) == Vector{UInt32}

    # UInt64
    v = Vector{UInt64}(undef, 2)
    v[1] = UInt64(1)
    v[2] = UInt64(0)
    @test v[1] == UInt64(1)
    @test v[2] == UInt64(0)
    @test typeof(v) == Vector{UInt64}
end

@testset "Typed array IndexStore: float types" begin
    # Float32
    v = Vector{Float32}(undef, 2)
    v[1] = Float32(3.14)
    v[2] = Float32(-1.0)
    @test v[1] == Float32(3.14)
    @test v[2] == Float32(-1.0)
    @test typeof(v) == Vector{Float32}
end

@testset "Typed array IndexStore: Bool" begin
    v = Vector{Bool}(undef, 2)
    v[1] = true
    v[2] = false
    @test v[1] == true
    @test v[2] == false
    @test typeof(v) == Vector{Bool}
end

@testset "Typed array IndexStore: overwrite" begin
    v = Vector{Int32}(undef, 1)
    v[1] = Int32(10)
    @test v[1] == Int32(10)
    v[1] = Int32(20)
    @test v[1] == Int32(20)

    v = Vector{Float32}(undef, 1)
    v[1] = Float32(1.0)
    @test v[1] == Float32(1.0)
    v[1] = Float32(2.0)
    @test v[1] == Float32(2.0)
end

true
