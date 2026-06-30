# Test isprimitivetype function (Issue #3767)
#
# After PR for #3767, isprimitivetype is a thin wrapper around the Rust
# `_isprimitivetype` intrinsic. The contract: T is primitive iff it is one
# of the 15 fixed-bit-width built-in types (Bool, Int*, UInt*, Float*, Char).
# Verified against `julia 1.12 -e 'isprimitivetype(T)'` for every assertion
# below.

using Test

mutable struct Mut3767
    x::Int64
end

struct Imm3767
    x::Int64
end

@testset "isprimitivetype - check if type is primitive" begin

    # All 15 primitive types
    @assert isprimitivetype(Bool)
    @assert isprimitivetype(Int8)
    @assert isprimitivetype(Int16)
    @assert isprimitivetype(Int32)
    @assert isprimitivetype(Int64)
    @assert isprimitivetype(Int128)
    @assert isprimitivetype(UInt8)
    @assert isprimitivetype(UInt16)
    @assert isprimitivetype(UInt32)
    @assert isprimitivetype(UInt64)
    @assert isprimitivetype(UInt128)
    @assert isprimitivetype(Float16)
    @assert isprimitivetype(Float32)
    @assert isprimitivetype(Float64)
    @assert isprimitivetype(Char)

    # String is NOT primitive in upstream Julia (struct internally)
    @assert !isprimitivetype(String)

    # Abstract numeric types are NOT primitive
    @assert !isprimitivetype(Number)
    @assert !isprimitivetype(Real)
    @assert !isprimitivetype(Integer)
    @assert !isprimitivetype(AbstractFloat)
    @assert !isprimitivetype(Any)

    # User-defined immutable struct → not primitive
    @assert !isprimitivetype(Imm3767)
    # User-defined mutable struct → not primitive
    @assert !isprimitivetype(Mut3767)

    @test (true)
end

true  # Test passed
