# rem/mod/div of typemin(T) by -1 for signed integers (Issue #9429)
#
# `rem(typemin(T), T(-1))` / `mod(typemin(T), T(-1))` must return 0 (upstream
# semantics) instead of panicking the VM with a Rust remainder overflow
# (`typemin % -1` overflows the machine remainder). The array-loaded `%` below
# exercises the VM's I64 fast path that used to abort the whole process.
# `div`/`fld`/`cld` on the same pairs must throw DivideError for EVERY signed
# width — the narrow widths (Int8/Int16/Int32) used to surface a wrong
# InexactError from the cast-through-Int64 narrowing conversion.
# All expected values below match upstream julia 1.12.

using Test

@testset "typemin(T) with -1 divisor (Issue #9429)" begin
    @testset "rem/mod return zero of the operand type" begin
        @test rem(typemin(Int8), Int8(-1)) === Int8(0)
        @test rem(typemin(Int16), Int16(-1)) === Int16(0)
        @test rem(typemin(Int32), Int32(-1)) === Int32(0)
        @test rem(typemin(Int64), Int64(-1)) === Int64(0)
        @test rem(typemin(Int128), Int128(-1)) === Int128(0)

        @test mod(typemin(Int8), Int8(-1)) === Int8(0)
        @test mod(typemin(Int16), Int16(-1)) === Int16(0)
        @test mod(typemin(Int32), Int32(-1)) === Int32(0)
        @test mod(typemin(Int64), Int64(-1)) === Int64(0)
        @test mod(typemin(Int128), Int128(-1)) === Int128(0)
    end

    @testset "% on runtime (array-loaded) operands does not panic" begin
        v = Int64[typemin(Int64), -1]
        @test v[1] % v[2] === Int64(0)
        w = Int8[typemin(Int8), Int8(-1)]
        @test w[1] % w[2] === Int8(0)
    end

    @testset "div/fld/cld throw DivideError" begin
        @test_throws DivideError div(typemin(Int8), Int8(-1))
        @test_throws DivideError div(typemin(Int16), Int16(-1))
        @test_throws DivideError div(typemin(Int32), Int32(-1))
        @test_throws DivideError div(typemin(Int64), Int64(-1))
        @test_throws DivideError div(typemin(Int128), Int128(-1))

        @test_throws DivideError fld(typemin(Int8), Int8(-1))
        @test_throws DivideError fld(typemin(Int64), Int64(-1))
        @test_throws DivideError cld(typemin(Int8), Int8(-1))
        @test_throws DivideError cld(typemin(Int64), Int64(-1))
    end

    @testset "ordinary div/rem/mod semantics unchanged" begin
        @test rem(-7, 3) === -1
        @test mod(-7, 3) === 2
        @test div(-7, 3) === -2
        @test fld(-7, 3) === -3
        @test cld(-7, 3) === -2
        @test div(typemin(Int8), Int8(1)) === typemin(Int8)
        @test rem(typemin(Int8), Int8(1)) === Int8(0)
        @test_throws DivideError div(5, 0)
        @test_throws DivideError rem(5, 0)
    end
end

true
