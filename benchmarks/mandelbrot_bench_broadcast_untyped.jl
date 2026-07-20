# Mandelbrot escape-time benchmark (broadcast) — untyped variant for sjulia stack VM
#
# Typed twin: benchmarks/mandelbrot_bench_broadcast.jl
# Type annotations are removed so the sjulia VM runs through the generic
# (untyped) interpreter path.

function mandelbrot_escape(c, maxiter)
    z = 0.0 + 0.0im
    for k in 1:maxiter
        if abs2(z) > 4.0
            return k - 1
        end
        z = z * z + c
    end
    return maxiter
end

function mandelbrot_grid(width, height, maxiter)
    xs = range(-2.0, 1.0; length=width)
    ys = range(1.2, -1.2; length=height)
    C = xs' .+ im .* ys
    counts = mandelbrot_escape.(C, maxiter)
    sum(counts)
end

function run_one(w, h, m)
    t0 = time_ns()
    r = mandelbrot_grid(w, h, m)
    t1 = time_ns()
    println(w, "x", h, " maxiter=", m, " total=", r, " t=", (t1 - t0) / 1.0e9)
end

mandelbrot_grid(50, 40, 50)
run_one(1700, 1360, 500)
