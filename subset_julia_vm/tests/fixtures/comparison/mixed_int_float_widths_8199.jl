# Exact mixed integer/float comparisons across ALL widths (Issue #8199,
# generalizing Issue #8187's Int64/Float64 case).
#
# `==` / `!=` / `<` / `<=` / `>` / `>=` (and `isequal` / `in` / tuple-`==`)
# between a fixed-width integer (Int8…Int128 / UInt8…UInt128) and a fixed IEEE
# float (Float16/Float32/Float64) must be value-based — never promote the integer
# to the float type (which rounds once |i| exceeds the float's exact-integer
# range: 2^53 for Float64, 2^24 for Float32). #8187 fixed Int64×Float64; this
# fixture pins the remaining widths.
#
# The final value is the logical AND of every check so the nextest harness
# actually validates the assertions (a bare `@testset` ending in `true` would be
# false-green until Issue #8191 is fixed).

using Test

checks = Bool[]
chk(cond) = (push!(checks, cond); cond)

@testset "exact mixed integer/float comparisons, all widths (Issue #8199)" begin
    # ---- UInt64 × Float64 (> 2^53) -------------------------------------------
    u = UInt64(9007199254740993)        # 2^53 + 1
    f = 9.007199254740992e15            # Float64(2^53)
    @test chk((u == f) == false)
    @test chk((f == u) == false)
    @test chk((u != f) == true)
    @test chk((u < f) == false)
    @test chk((u > f) == true)
    @test chk((u <= f) == false)
    @test chk((u >= f) == true)

    # ---- Int128 / UInt128 × Float64 ------------------------------------------
    i128 = Int128(9007199254740993)
    @test chk((i128 == f) == false)
    @test chk((i128 <= f) == false)
    @test chk((i128 > f) == true)
    u128 = UInt128(9007199254740993)
    @test chk((u128 == f) == false)
    @test chk((f < u128) == true)

    # A huge Int128 far beyond Float64's exact range still orders correctly.
    big_i = Int128(1) << 100
    @test chk((big_i > 1.0e30) == true)
    @test chk((big_i == 1.0e30) == false)

    # ---- Float32 mixes (> 2^24) ----------------------------------------------
    # 2^24 = 16777216 is the largest consecutive integer exactly representable
    # in Float32; 2^24 + 1 rounds to 2^24 under naive widening.
    f32 = Float32(16777216.0)
    @test chk((Int64(16777217) == f32) == false)
    @test chk((f32 == Int64(16777217)) == false)
    @test chk((Int64(16777216) == f32) == true)
    @test chk((Int64(16777217) > f32) == true)
    @test chk((UInt32(16777217) > f32) == true)
    @test chk((Int64(16777215) < f32) == true)

    # ---- Float16 mixes (range is tiny, but stays value-based) ----------------
    h = Float16(2.0)
    @test chk((Int64(2) == h) == true)
    @test chk((Int64(3) == h) == false)
    @test chk((Int64(3) > h) == true)
    @test chk((Int16(-5) < Float16(-4.5)) == true)

    # ---- Small widths keep working (exact via promotion) ---------------------
    @test chk((Int32(5) == 5.0) == true)
    @test chk((UInt8(3) < 3.5) == true)
    @test chk((Int8(-2) <= -2.0) == true)

    # ---- isequal: value-based AND signed-zero aware (an integer is +0) -------
    @test chk(isequal(u, f) == false)
    @test chk(isequal(UInt64(0), -0.0) == false)
    @test chk(isequal(Int128(0), 0.0) == true)
    @test chk(isequal(UInt32(7), Float32(7.0)) == true)

    # ---- membership / tuple `==` share the exact comparison ------------------
    @test chk((u in [f, 1.0]) == false)
    @test chk((UInt8(2) in [1.0, 2.0, 3.0]) == true)
    @test chk(((u,) == (f,)) == false)
    @test chk(((Int128(2),) == (2.0,)) == true)
end

all(checks)
