# Issue #9654: the SROA'd (#9198 S2) ComplexF64 Mandelbrot escape loop — a
# counted loop whose escape path is an early `return` — executes on the native
# typed-loop path (`TypedLoopOp::ReturnI64`). This locks the semantics of that
# path: escape counts, the early-return value (`k - 1`), the loop-exhausted
# value (`maxiter`), and a small-grid checksum must stay identical to upstream
# Julia and to the per-instruction interpreter.

using Test

function mandel_point_9654(c::ComplexF64, maxiter::Int64)::Int64
    z = 0.0 + 0.0im
    for k in 1:maxiter
        if abs2(z) > 4.0
            return k - 1
        end
        z = z * z + c
    end
    return maxiter
end

# Real-decomposed twin of the same kernel: exercises the typed-loop early
# return without any Complex involvement (the pure-Float64 escape-loop class).
function mandel_point_real_9654(cr::Float64, ci::Float64, maxiter::Int64)::Int64
    zr = 0.0
    zi = 0.0
    for k in 1:maxiter
        if zr * zr + zi * zi > 4.0
            return k - 1
        end
        t = zr * zr - zi * zi + cr
        zi = 2.0 * zr * zi + ci
        zr = t
    end
    return maxiter
end

@testset "escape counts: early return vs loop exhaustion" begin
    # Interior point: never escapes -> maxiter via the normal loop exit.
    @test mandel_point_9654(0.0 + 0.0im, 50) == 50
    # Far exterior point: |c| large -> escapes at k = 2 -> returns 1.
    @test mandel_point_9654(2.0 + 2.0im, 50) == 1
    # Boundary-ish point with a known count.
    @test mandel_point_9654(0.3 + 0.5im, 500) == 500
    @test mandel_point_9654(-0.75 + 0.3im, 500) == 11
    # maxiter = 0: the loop body never runs.
    @test mandel_point_9654(2.0 + 2.0im, 0) == 0
end

@testset "Complex form and real-decomposed form agree" begin
    for (cr, ci) in ((0.0, 0.0), (2.0, 2.0), (-0.75, 0.3), (0.3, 0.5), (-2.0, 1.2))
        @test mandel_point_9654(cr + ci * im, 100) ==
              mandel_point_real_9654(cr, ci, 100)
    end
end

@testset "small-grid checksum matches upstream" begin
    total = 0
    for y in 1:40
        ci = -1.2 + 2.4 * (y - 1) / 39
        for x in 1:40
            cr = -2.0 + 3.0 * (x - 1) / 39
            total += mandel_point_9654(cr + ci * im, 100)
        end
    end
    # Value verified against upstream julia (see fixture parity check).
    @test total == 39114
end

true
