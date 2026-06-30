# Test that public numeric conversion functions reach Pure Julia
# method dispatch (Issue #3727).
#
# Migration scope:
#   - `complex(...)` is Pure Julia (base/complex.jl). The dead
#     `BuiltinId::Complex` inventory entry was removed.
#   - `float(...)` is Pure Julia (base/number.jl, base/rational.jl,
#     base/complex.jl). `BuiltinId::FloatConv` is no longer reachable
#     from any public name.
#   - `signed` / `unsigned` route through Pure Julia methods on
#     base/number.jl for the supported integer/Bool types. The Rust
#     BuiltinId fallback is preserved only for inputs that have no
#     Pure Julia method (e.g. Float64).

using Test

# User-defined wrapper / first-class function paths
float_via_wrapper(x) = float(x)
complex_via_wrapper(r, i) = complex(r, i)
signed_via_wrapper(x) = signed(x)
unsigned_via_wrapper(x) = unsigned(x)
apply1(f, x) = f(x)
apply2(f, x, y) = f(x, y)

@testset "Pure Julia dispatch for numeric conversions (Issue #3727)" begin
    # === complex(...) → base/complex.jl ===
    c1 = complex(3, 4)
    @test (real(c1)) == 3
    @test (imag(c1)) == 4

    c2 = complex(3.0, 4.0)
    @test (real(c2)) == 3.0
    @test (imag(c2)) == 4.0

    # Single-argument complex
    c3 = complex(5.0)
    @test (real(c3)) == 5.0
    @test (imag(c3)) == 0.0

    # Wrapper / first-class function paths
    cw = complex_via_wrapper(7, 8)
    @test (real(cw)) == 7
    @test (imag(cw)) == 8

    ca = apply2(complex, 9.0, 10.0)
    @test (real(ca)) == 9.0
    @test (imag(ca)) == 10.0

    # === float(...) → Pure Julia ===
    @test (float(1)) == 1.0
    @test (float(Int32(2))) == 2.0
    @test (float(UInt64(3))) == 3.0
    @test (float(true)) == 1.0
    @test (float_via_wrapper(4.0)) == 4.0
    @test (apply1(float, 5)) == 5.0

    # === signed / unsigned → Pure Julia for supported types ===
    @test (signed(UInt8(5))) == Int8(5)
    @test (signed(UInt32(5))) == Int32(5)
    @test (signed(Int64(5))) == Int64(5)         # identity
    @test (signed(true)) == 1
    @test (unsigned(Int8(-1))) == UInt8(0xff)
    @test (unsigned(Int64(5))) == reinterpret(UInt64, Int64(5))
    @test (unsigned_via_wrapper(Int8(-2))) == UInt8(0xfe)
    @test (signed_via_wrapper(UInt32(7))) == Int32(7)
    @test (apply1(signed, UInt8(9))) == Int8(9)
end

true
