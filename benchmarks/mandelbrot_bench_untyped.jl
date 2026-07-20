# Mandelbrot escape-time benchmark (for-loop, Complex) — untyped variant
#
# Same algorithm as mandelbrot_bench_for.jl, but `mandel_point` is completely
# untyped (`c, maxiter::Int64`) so it exercises the runtime specialization
# path for ComplexF64 operations. Used as the baseline / retirement fixture
# for Issue #10530.

function mandel_point(c, maxiter::Int64)::Int64
    z = 0.0 + 0.0im
    for k in 1:maxiter
        if abs2(z) > 4.0
            return k - 1
        end
        z = z * z + c
    end
    return maxiter
end

function mandel_count(width::Int64, height::Int64, maxiter::Int64)::Int64
    total = 0
    for y in 1:height
        ci = -1.2 + 2.4 * (y - 1) / (height - 1)
        for x in 1:width
            cr = -2.0 + 3.0 * (x - 1) / (width - 1)
            total += mandel_point(cr + ci * im, maxiter)
        end
    end
    total
end

function run_one(w::Int64, h::Int64, m::Int64)
    t0 = time_ns()
    r = mandel_count(w, h, m)
    t1 = time_ns()
    println(w, "x", h, " maxiter=", m, " total=", r, " t=", (t1 - t0) / 1.0e9)
end

mandel_count(200, 200, 100)   # warmup
run_one(1500, 1500, 500)
