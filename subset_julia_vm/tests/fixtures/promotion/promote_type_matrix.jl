# Systematic promote_type matrix over the numeric type lattice (Issue #5070).
#
# Verifies that promote_type(T, S) matches upstream Julia exactly across
# Int8/16/32/64/128, UInt8/16/32/64/128, Bool, Float16/32/64, BigInt, BigFloat,
# Rational and Complex, AND that promotion is symmetric: promote_type(T, S) ===
# promote_type(S, T) for every pair. Verified against `julia` (1.12) first.
#
# A single flat @testset keeps the testset summary shape identical between
# sjulia and upstream so scripts/fixture_julia_parity.sh reports clean parity.

using Test

# Symmetry over the full lattice, reduced to one boolean so the assertion count
# stays identical between sjulia and upstream.
function all_symmetric(types)
    for T in types
        for S in types
            if promote_type(T, S) !== promote_type(S, T)
                return false
            end
        end
    end
    return true
end

@testset "promote_type matrix (Issue #5070)" begin
    # integer widening (signed)
    @test promote_type(Int8, Int16) === Int16
    @test promote_type(Int16, Int8) === Int16
    @test promote_type(Int8, Int128) === Int128
    @test promote_type(Int64, Int128) === Int128
    @test promote_type(Int32, Int64) === Int64
    @test promote_type(Int8, Int8) === Int8

    # Bool promotes to the other Number
    @test promote_type(Bool, Int) === Int
    @test promote_type(Int, Bool) === Int
    @test promote_type(Bool, Int128) === Int128
    @test promote_type(Bool, UInt8) === UInt8
    @test promote_type(Bool, Float64) === Float64
    @test promote_type(Bool, BigInt) === BigInt

    # unsigned widening and signed/unsigned mixing
    @test promote_type(UInt8, UInt16) === UInt16
    @test promote_type(UInt8, UInt128) === UInt128
    @test promote_type(UInt32, UInt64) === UInt64
    @test promote_type(Int8, UInt8) === UInt8          # same width: unsigned wins
    @test promote_type(UInt8, Int8) === UInt8
    @test promote_type(Int64, UInt64) === UInt64
    @test promote_type(Int16, UInt8) === Int16         # wider signed absorbs narrower unsigned
    @test promote_type(Int64, UInt32) === Int64
    @test promote_type(Int128, UInt64) === Int128
    @test promote_type(UInt16, Int8) === UInt16        # wider unsigned absorbs narrower signed
    @test promote_type(UInt64, Int32) === UInt64

    # float promotion
    @test promote_type(Int, Float64) === Float64
    @test promote_type(Float64, Int) === Float64
    @test promote_type(Float32, Int) === Float32
    @test promote_type(Int8, Float16) === Float16
    @test promote_type(Float32, Float64) === Float64
    @test promote_type(Float16, Float32) === Float32
    @test promote_type(UInt128, Float32) === Float32
    @test promote_type(Int128, Float64) === Float64

    # BigInt and BigFloat
    @test promote_type(BigInt, Int64) === BigInt
    @test promote_type(Int8, BigInt) === BigInt
    @test promote_type(BigInt, UInt128) === BigInt
    @test promote_type(BigInt, Int128) === BigInt
    @test promote_type(BigInt, Float64) === BigFloat
    @test promote_type(Float16, BigInt) === BigFloat
    @test promote_type(BigFloat, Int8) === BigFloat
    @test promote_type(BigFloat, UInt128) === BigFloat
    @test promote_type(BigFloat, BigInt) === BigFloat
    @test promote_type(BigFloat, Float64) === BigFloat

    # Rational
    @test promote_type(Rational{Int64}, Int64) === Rational{Int64}
    @test promote_type(Rational{Int8}, Int16) === Rational{Int16}
    @test promote_type(Rational{Int8}, UInt16) === Rational{UInt16}
    @test promote_type(Rational{Int64}, Int128) === Rational{Int128}
    @test promote_type(Rational{Int8}, Rational{Int16}) === Rational{Int16}
    @test promote_type(Rational{Int64}, BigInt) === Rational{BigInt}
    @test promote_type(Rational{BigInt}, Rational{Int64}) === Rational{BigInt}
    @test promote_type(Rational{Int64}, Float64) === Float64       # float beats rational
    @test promote_type(Rational{Int8}, Float16) === Float16
    @test promote_type(Rational{BigInt}, Float64) === BigFloat     # prior bug returned Float
    @test promote_type(Float32, Rational{BigInt}) === BigFloat

    # Complex
    @test promote_type(Complex{Float64}, Complex{Int64}) === Complex{Float64}
    @test promote_type(Complex{Int64}, Complex{Bool}) === Complex{Int64}
    @test promote_type(Complex{Float32}, Complex{Float64}) === Complex{Float64}
    @test promote_type(Int8, Complex{Float64}) === Complex{Float64}
    @test promote_type(Complex{Bool}, Float64) === Complex{Float64}
    @test promote_type(Int, Complex{Bool}) === Complex{Int}
    @test promote_type(Complex{Int64}, Float64) === Complex{Float64}

    # transitive multi-arg
    @test promote_type(Int8, Float16, Float32) === Float32
    @test promote_type(Bool, Int8, Float32, Float64) === Float64
    @test promote_type(Int8, Int16, Int32) === Int32

    # no rule falls back to typejoin
    @test promote_type(Int, String) === Any

    # symmetry: promote_type(T, S) === promote_type(S, T) for the full lattice
    @test all_symmetric([Bool, Int8, Int16, Int32, Int64, Int128,
                         UInt8, UInt16, UInt32, UInt64, UInt128,
                         Float16, Float32, Float64, BigInt, BigFloat,
                         Rational{Int8}, Rational{Int64}, Rational{BigInt},
                         Complex{Bool}, Complex{Int64}, Complex{Float64}])
end

true
