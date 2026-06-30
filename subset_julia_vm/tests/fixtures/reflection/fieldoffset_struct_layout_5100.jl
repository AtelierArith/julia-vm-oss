using Test

# Issue #5100: fieldoffset(T, i) and struct memory-layout offset/alignment
# for isbits / immutable structs.

struct FOTwoInt5100
    x::Int64
    y::Int64
end

struct FOMixed5100
    a::Int8
    b::Int64
    c::Int16
end

struct FOPacked5100
    a::Int8
    b::Int8
    c::Int8
end

struct FOFloats5100
    x::Float32
    y::Float64
end

struct FOSingle5100
    x::Int64
end

struct FOCharBool5100
    a::Char
    b::Bool
end

# Nested isbits struct: FOTwoInt5100 has alignment 8 (max field align), size 16.
struct FONested5100
    p::FOTwoInt5100
    z::Int8
end

# Odd-sized inner struct (size 3, alignment 1): an outer Int8 then this packs
# at offset 1, not 4.
struct FOPackedInner5100
    x::Int8
    y::FOPacked5100
end

# Inner struct of non-power-of-two size 24 but alignment 8.
struct FOBig24_5100
    a::Int64
    b::Int64
    c::Int64
end

struct FOWrapBig24_5100
    x::Int8
    y::FOBig24_5100
end

# Mutable struct: fields are boxed/by-pointer, alignment 8.
mutable struct FOMutable5100
    x::Int64
    y::Bool
end

@testset "fieldoffset / struct layout for isbits structs (Issue #5100)" begin
    # Two Int64 fields: first at 0, second at 8.
    @test fieldoffset(FOTwoInt5100, 1) == UInt64(0)
    @test fieldoffset(FOTwoInt5100, 2) == UInt64(8)
    @test sizeof(FOTwoInt5100) == 16
    @test isbitstype(FOTwoInt5100)

    # Mixed-size fields with alignment padding.
    @test fieldoffset(FOMixed5100, 1) == UInt64(0)
    @test fieldoffset(FOMixed5100, 2) == UInt64(8)
    @test fieldoffset(FOMixed5100, 3) == UInt64(16)
    @test sizeof(FOMixed5100) == 24

    # Packed Int8 fields: no padding.
    @test fieldoffset(FOPacked5100, 1) == UInt64(0)
    @test fieldoffset(FOPacked5100, 2) == UInt64(1)
    @test fieldoffset(FOPacked5100, 3) == UInt64(2)
    @test sizeof(FOPacked5100) == 3

    # Float32 then Float64: second aligned to 8.
    @test fieldoffset(FOFloats5100, 1) == UInt64(0)
    @test fieldoffset(FOFloats5100, 2) == UInt64(8)
    @test sizeof(FOFloats5100) == 16

    # Single field: offset 0.
    @test fieldoffset(FOSingle5100, 1) == UInt64(0)
    @test sizeof(FOSingle5100) == 8

    # Char (4 bytes) then Bool (1 byte).
    @test fieldoffset(FOCharBool5100, 1) == UInt64(0)
    @test fieldoffset(FOCharBool5100, 2) == UInt64(4)
    @test sizeof(FOCharBool5100) == 8

    # Nested isbits struct aligns to 8 (its max field align), not its size.
    @test fieldoffset(FONested5100, 1) == UInt64(0)
    @test fieldoffset(FONested5100, 2) == UInt64(16)
    @test sizeof(FONested5100) == 24
    @test isbitstype(FONested5100)

    # Odd-sized inner struct (size 3, align 1) packs right after the Int8.
    @test fieldoffset(FOPackedInner5100, 1) == UInt64(0)
    @test fieldoffset(FOPackedInner5100, 2) == UInt64(1)
    @test sizeof(FOPackedInner5100) == 4

    # 24-byte inner struct has alignment 8, so it lands at offset 8.
    @test fieldoffset(FOWrapBig24_5100, 1) == UInt64(0)
    @test fieldoffset(FOWrapBig24_5100, 2) == UInt64(8)
    @test sizeof(FOWrapBig24_5100) == 32

    # Mutable struct: pointer-width fields.
    @test fieldoffset(FOMutable5100, 1) == UInt64(0)
    @test fieldoffset(FOMutable5100, 2) == UInt64(8)

    # First field is always at offset 0.
    @test fieldoffset(FOTwoInt5100, 1) == UInt64(0)
    @test fieldoffset(FOMixed5100, 1) == UInt64(0)
    @test fieldoffset(FOWrapBig24_5100, 1) == UInt64(0)

    # Return type is UInt64.
    @test typeof(fieldoffset(FOTwoInt5100, 1)) === UInt64
end

true
