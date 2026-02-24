# Test typed local variable declarations (x::Type = value)

using Test

function test_typed_locals()::Float64
    x::Float64 = 1.0
    y::Float64 = 2.0
    z::Float64 = x + y
    return z
end

function mandelbrot_escape(cr::Float64, ci::Float64, maxiter::Int64)::Int64
    zr::Float64 = 0.0
    zi::Float64 = 0.0
    for k in 1:maxiter
        if (zr * zr + zi * zi) > 4.0
            return k
        end
        new_zr::Float64 = zr * zr - zi * zi + cr
        new_zi::Float64 = 2.0 * zr * zi + ci
        zr = new_zr
        zi = new_zi
    end
    return maxiter
end

@testset "Typed local variable declarations" begin
    @test test_typed_locals() == 3.0
    @test mandelbrot_escape(0.0, 0.0, 100) == 100  # Origin, inside set
    @test mandelbrot_escape(2.0, 0.0, 100) < 100   # Outside set, escapes
end

true
