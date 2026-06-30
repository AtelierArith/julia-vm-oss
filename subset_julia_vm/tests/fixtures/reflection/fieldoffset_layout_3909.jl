using Test

struct FieldOffsetBits3909
    x::Int64
    y::Bool
    z::Int16
end

struct FieldOffsetRefs3909
    name::String
    flag::Bool
    value::Int64
end

mutable struct FieldOffsetMutable3909
    x::Int64
    y::Bool
end

@testset "fieldoffset uses runtime type layout metadata (Issue #3909)" begin
    @test fieldoffset(FieldOffsetBits3909, 1) == UInt64(0)
    @test fieldoffset(FieldOffsetBits3909, 2) == UInt64(8)
    @test fieldoffset(FieldOffsetBits3909, 3) == UInt64(10)

    @test fieldoffset(FieldOffsetRefs3909, 1) == UInt64(0)
    @test fieldoffset(FieldOffsetRefs3909, 2) == UInt64(8)
    @test fieldoffset(FieldOffsetRefs3909, 3) == UInt64(16)

    @test fieldoffset(FieldOffsetMutable3909, 1) == UInt64(0)
    @test fieldoffset(FieldOffsetMutable3909, 2) == UInt64(8)

    @test fieldoffset(LineNumberNode, 2) == UInt64(8)
    @test fieldoffset(GlobalRef, 3) == UInt64(16)
    @test typeof(fieldoffset(FieldOffsetBits3909, 1)) === UInt64
end

true
