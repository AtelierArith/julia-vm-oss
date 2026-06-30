# Issue #6746 (#6730-1): @sprintf / sprintf is a pure-Julia C-style Printf engine
# (base/printf.jl). It parses flags / width / .precision / conversion and lays out
# integers, strings and chars itself; the float conversions (%f %e %E %g %G)
# delegate to the Rust _printf_fmt_float boundary (the Ryu float→string entry).
# This fixes the old behavior, which ignored width/precision/flags and dropped the
# default float precision. Uses the standard @sprintf macro so values are checked
# against upstream julia 1.12.

using Printf
using Test

@testset "integer width / flags (Issue #6746)" begin
    @test @sprintf("%d", 42) == "42"
    @test @sprintf("%5d", 42) == "   42"
    @test @sprintf("%-5d", 42) == "42   "
    @test @sprintf("%05d", 42) == "00042"
    @test @sprintf("%+d", 42) == "+42"
    @test @sprintf("% d", 42) == " 42"
    @test @sprintf("%05d", -7) == "-0007"
    @test @sprintf("%.3d", 5) == "005"
end

@testset "hex / octal (Issue #6746)" begin
    @test @sprintf("%x", 255) == "ff"
    @test @sprintf("%X", 255) == "FF"
    @test @sprintf("%#x", 255) == "0xff"
    @test @sprintf("%08x", 255) == "000000ff"
    @test @sprintf("%o", 64) == "100"
    @test @sprintf("%#o", 64) == "0100"
end

@testset "float precision / width / flags (Issue #6746)" begin
    @test @sprintf("%f", 3.14159) == "3.141590"   # default precision 6
    @test @sprintf("%.2f", 3.14159) == "3.14"
    @test @sprintf("%8.2f", 3.14159) == "    3.14"
    @test @sprintf("%-8.2f", 3.14159) == "3.14    "
    @test @sprintf("%08.2f", 3.14159) == "00003.14"
    @test @sprintf("%+.1f", 2.5) == "+2.5"
    @test @sprintf("%f", -0.0) == "-0.000000"
end

@testset "scientific / general (Issue #6746)" begin
    @test @sprintf("%e", 12345.678) == "1.234568e+04"
    @test @sprintf("%.2e", 12345.678) == "1.23e+04"
    @test @sprintf("%E", 0.00012) == "1.200000E-04"
    @test @sprintf("%g", 100000.0) == "100000"
    @test @sprintf("%g", 1000000.0) == "1e+06"
    @test @sprintf("%g", 0.0001) == "0.0001"
    @test @sprintf("%g", 3.14159) == "3.14159"
end

@testset "strings / chars / literals (Issue #6746)" begin
    @test @sprintf("%s", "hi") == "hi"
    @test @sprintf("%5s", "hi") == "   hi"
    @test @sprintf("%-5s", "hi") == "hi   "
    @test @sprintf("%.1s", "hello") == "h"
    @test @sprintf("%c", 'A') == "A"
    @test @sprintf("%c", 66) == "B"
    @test @sprintf("%d%% done", 50) == "50% done"
    @test @sprintf("%d and %s", 3, "x") == "3 and x"
end

true
