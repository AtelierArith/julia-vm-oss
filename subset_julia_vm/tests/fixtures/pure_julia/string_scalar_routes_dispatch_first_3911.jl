using Test

struct StringScalarDispatchBox3911
    n::Int64
end

function Base.bitstring(b::StringScalarDispatchBox3911)
    return "box-bitstring-$(b.n)"
end

function Base.codepoint(b::StringScalarDispatchBox3911)
    return UInt32(b.n + 100)
end

function Base.isnumeric(b::StringScalarDispatchBox3911)
    return b.n == 7
end

function Base.unescape_string(b::StringScalarDispatchBox3911)
    return "box-unescape-$(b.n)"
end

function Base.tryparse(::Type{StringScalarDispatchBox3911}, s::String)
    return StringScalarDispatchBox3911(parse(Int64, s) + 1)
end

function Base.parse(::Type{StringScalarDispatchBox3911}, s::String)
    return StringScalarDispatchBox3911(parse(Int64, s) + 2)
end

box = StringScalarDispatchBox3911(7)

@test Base.bitstring(box) == "box-bitstring-7"
@test Base.codepoint(box) == UInt32(107)
@test Base.isnumeric(box) == true
@test Base.unescape_string(box) == "box-unescape-7"
@test Base.tryparse(StringScalarDispatchBox3911, "40").n == 41
@test Base.parse(StringScalarDispatchBox3911, "40").n == 42

# Primitive fallbacks still use their retained Rust/Pure Julia paths.
@test bitstring(Int8(3)) == "00000011"
@test codepoint('A') == UInt32(65)
@test isnumeric('9') == true
@test unescape_string("\\n") == "\n"
@test parse(Int64, "42") == 42
@test tryparse(Int64, "42") == 42

true
