# 2-field isbits-struct slot-pair SROA — Issue #9198 S3 (generalizes S2).
#
# S3 qualifies three new families for the `compile::complex_sroa` unboxing:
#   1. `im`-based literal initializers whose coefficients are provably Float64
#      (`z = 0.0 + 0.0im`, `c = cr + ci*im`) — Complex{Float64}, not Complex{Int}.
#   2. a boxed `::ComplexF64` PARAMETER used as a decomposed operand in the loop
#      (`z = z*z + c`), its re/im hoisted to f64 locals at entry.
#   3. any user immutable struct with exactly two Float64 fields — construction
#      `T(a,b)` and field reads `p.x`/`p.y` unbox (no built-in arithmetic).
#
# Every result here must be byte-identical to the boxed form (upstream Julia).
using Test

# --- (1) im-literal init + (2) ComplexF64 param operand: the mandelbrot kernel
# spelling. `z = 0.0 + 0.0im`; `c` is a boxed ComplexF64 parameter.
function mandel_point(c::ComplexF64, maxiter::Int64)::Int64
    z = 0.0 + 0.0im
    k = 0
    while k < maxiter
        if abs2(z) > 4.0
            return k
        end
        z = z * z + c
        k = k + 1
    end
    maxiter
end

function mandel_count(width::Int64, height::Int64, maxiter::Int64)::Int64
    total = 0
    y = 1
    while y <= height
        ci = -1.2 + 2.4 * (y - 1) / (height - 1)
        x = 1
        while x <= width
            cr = -2.0 + 3.0 * (x - 1) / (width - 1)
            total = total + mandel_point(cr + ci * im, maxiter)
            x = x + 1
        end
        y = y + 1
    end
    total
end

# --- (1) im-literal accumulation: z = 0.0 + 0.0im, real coefficient scaling.
function im_accum(n::Int64)::Float64
    z = 0.0 + 0.0im
    c = 0.1 + 0.2im
    i = 0
    while i < n
        z = z * z + c
        i = i + 1
    end
    real(z) + imag(z)
end

# --- (3) user 2-field Float64 immutable struct: construct + field reads unbox.
struct V2
    x::Float64
    y::Float64
end

function march(n::Int64)::Float64
    p = V2(0.0, 0.0)
    i = 0
    while i < n
        p = V2(p.x + 1.0, p.y + 2.0)
        i = i + 1
    end
    p.x + p.y
end

# --- (3) user struct escape: returning the struct materializes it back (boxed).
function march_ret(n::Int64)
    p = V2(1.0, 0.5)
    i = 0
    while i < n
        p = V2(p.x * 2.0, p.y + p.x)
        i = i + 1
    end
    p
end

@testset "S3 mandelbrot (im-literal init + ComplexF64 param)" begin
    @test mandel_count(30, 20, 50) == 8278
    @test mandel_point(0.3 + 0.6im, 50) == 15
    @test mandel_point(-0.5 + 0.5im, 50) == 50
    @test mandel_point(2.0 + 2.0im, 50) == 1
end

@testset "S3 im-literal accumulation" begin
    @test im_accum(10) == 0.2763715796999337
    @test im_accum(0) == 0.0
end

@testset "S3 user 2-field Float64 struct construct + field reads" begin
    @test march(5) == 15.0
    @test march(0) == 0.0
    @test march(3) == 9.0
end

@testset "S3 user struct escape materializes" begin
    r = march_ret(4)
    @test r isa V2
    @test r.x == 16.0
    @test r.y == 15.5
end

true
