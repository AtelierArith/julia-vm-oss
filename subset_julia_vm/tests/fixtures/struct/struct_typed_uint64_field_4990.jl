# Typed struct constructors preserve declared field value types (Issue #4990)

using Test

struct FooUInt64Field
    x::UInt64
end

struct MixedTypedFields
    a::UInt8
    b::Int32
    c::Float32
end

@testset "typed struct field preserves UInt64 (Issue #4990)" begin
    f = FooUInt64Field(UInt64(2))
    @test typeof(f.x) == UInt64
    @test f.x == UInt64(2)
    @test f.x === UInt64(2)
end

@testset "typed struct field coerces declared types (Issue #4990)" begin
    # Integer literal arguments are converted to the declared field types,
    # matching Julia's default constructor convert() behavior.
    m = MixedTypedFields(1, 2, 3)
    @test typeof(m.a) == UInt8
    @test typeof(m.b) == Int32
    @test typeof(m.c) == Float32
    @test m.a === UInt8(1)
    @test m.b === Int32(2)
    @test m.c === Float32(3)
end

true
