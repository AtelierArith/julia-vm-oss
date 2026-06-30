# Issue #7953: a typed array literal `T[elems...]` must `convert(T, x)` each
# element to the declared element type `T` before storing, exactly like upstream
# Julia (whose `T[a, b, ...]` lowers to `a = Vector{T}(undef, n); a[i] = vals[i]`,
# and `setindex!` does `convert(T, x)`).
#
# Plain Int literals already match `Int64` storage, so the missing per-element
# convert was invisible until a UInt-family hex literal (`0x30::UInt8`,
# `0x663::UInt16`, ...) is mixed into a signed/float typed literal: sjulia tried
# to store the raw `UInt8` into an `Int64` array and failed with
#   "Cannot store U8 in I64 array".
#
# Routing each element through `convert(T, x)` also makes out-of-range elements
# raise `InexactError` (matching upstream) instead of silently truncating.
using Test

@testset "Issue #7953: typed array literal converts UInt hex elements" begin
    # The exact repro from the issue.
    @test Int[0x30, 0x39] == [48, 57]
    @test Int[0x30, 0x39] isa Vector{Int64}
    @test eltype(Int[0x30, 0x39]) === Int64

    # Wider hex literals (UInt16) convert into the declared Int element type.
    @test Int[0x663] == [1635]
    @test Int[0x663] isa Vector{Int64}

    # Mixed hex (UInt8) and decimal (Int64) elements.
    @test Int[0x30, 49] == [48, 49]
    @test Int[0x30, 49] isa Vector{Int64}

    # Narrower signed targets still convert in-range hex elements.
    @test Int8[0x30] == Int8[48]
    @test Int8[0x30] isa Vector{Int8}
    @test Int32[0x30] == Int32[48]
    @test Int32[0x30] isa Vector{Int32}
    @test Int128[0x30] == Int128[48]
    @test eltype(Int128[0x30]) === Int128

    # Unsigned targets: decimal Int literals convert into the UInt element type.
    @test UInt8[1, 2] == UInt8[0x01, 0x02]
    @test UInt8[1, 2] isa Vector{UInt8}
    # ... and a mix of narrower hex literals widens into UInt64.
    @test UInt[0x30, 0x663] == UInt64[0x30, 0x663]
    @test UInt[0x30, 0x663] isa Vector{UInt64}

    # Float targets convert hex elements too.
    @test Float64[0x30] == [48.0]
    @test Float64[0x30] isa Vector{Float64}
    @test Float32[0x30] == Float32[48.0]
    @test Float32[0x30] isa Vector{Float32}

    # 2-D typed matrix literals convert per element as well.
    M = Int[0x1 0x2; 0x3 0x4]
    @test M == [1 2; 3 4]
    @test M isa Matrix{Int64}

    # Regression: pure decimal literals keep working unchanged.
    @test Int[48, 57] == [48, 57]
    @test Int[48, 57] isa Vector{Int64}

    # Out-of-range elements raise InexactError (faithful convert semantics),
    # instead of silently truncating.
    @test_throws InexactError Int[0xffffffffffffffff]
    @test_throws InexactError Int8[0xc8]
    @test_throws InexactError UInt8[300]
end

true
