# Mandelbrot VM execution benchmark (Issue #4301).
# This file is intentionally scalar-heavy and allocation-light so it primarily
# exercises VM numeric ops, branches, local load/store, and call overhead.

function mandel_point(cr::Float64, ci::Float64, maxiter::Int64)::Int64
    zr = 0.0
    zi = 0.0
    iter = 0
    while zr * zr + zi * zi <= 4.0 && iter < maxiter
        zr2 = zr * zr - zi * zi + cr
        zi = 2.0 * zr * zi + ci
        zr = zr2
        iter = iter + 1
    end
    iter
end

function mandel_count(width::Int64, height::Int64, maxiter::Int64)::Int64
    total = 0
    y = 1
    while y <= height
        ci = (2.0 * Float64(y) / Float64(height)) - 1.0
        x = 1
        while x <= width
            cr = (3.5 * Float64(x) / Float64(width)) - 2.5
            total = total + mandel_point(cr, ci, maxiter)
            x = x + 1
        end
        y = y + 1
    end
    total
end

function main()
    # Keep these constants in sync with benchmarks/scripts/run_vm_mandelbrot.sh.
    # Start with a deliberately small VM workload; sjulia VM execution is slow
    # enough that larger sizes should be introduced only after profiling.
    result = mandel_count(120, 80, 60)
    println(result)
end

main()
