using Test

struct StringCodeUnitDispatchBox3911
    n::Int64
end

function Base.ncodeunits(b::StringCodeUnitDispatchBox3911)
    return b.n + 1000
end

function Base.codeunit(b::StringCodeUnitDispatchBox3911)
    return b.n + 2000
end

function Base.codeunit(b::StringCodeUnitDispatchBox3911, i::Int64)
    return b.n + i + 3000
end

function Base.codeunits(b::StringCodeUnitDispatchBox3911)
    return [b.n, b.n + 1, b.n + 2]
end

box = StringCodeUnitDispatchBox3911(11)

@test Base.ncodeunits(box) == 1011
@test Base.codeunit(box) == 2011
@test Base.codeunit(box, 7) == 3018
@test Base.codeunits(box) == [11, 12, 13]

# Primitive String calls still use the Rust byte-level fallback.
@test Base.ncodeunits("aé") == 3
@test Base.codeunit("aé", 1) == 0x61
@test Base.codeunits("aé") == [0x61, 0xc3, 0xa9]

true
