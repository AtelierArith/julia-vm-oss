# Mandelbrot escape-time benchmark (for-loop, Complex) — untyped variant for sjulia stack VM
#
# Typed twin: benchmarks/mandelbrot_bench_for.jl
# All type annotations are removed so the sjulia VM runs through the generic
# (untyped) interpreter path.

function mandel_point(c, maxiter)
    z = 0.0 + 0.0im
    for k in 1:maxiter
        if abs2(z) > 4.0
            return k - 1
        end
        z = z * z + c
    end
    return maxiter
end

function mandel_count(width, height, maxiter)
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

function run_one(w, h, m)
    t0 = time_ns()
    r = mandel_count(w, h, m)
    t1 = time_ns()
    println(w, "x", h, " maxiter=", m, " total=", r, " t=", (t1 - t0) / 1.0e9)
end

mandel_count(200, 200, 100)
run_one(1500, 1500, 500)
