# Mandelbrot escape-time — AoT acceptance fixture (Issue #8639).
#
# One of the three ADR_BACKEND_STRATEGY.md acceptance programs (coprime pi /
# Aizawa / Mandelbrot): the AoT backend must compile AND run this program with
# output identical to upstream Julia. Mandelbrot is the gate proving sjulia
# handles **Complex arithmetic**: the hot loop is `z = z * z + c` on
# ComplexF64 values, not a real/imag decomposition. Scalar `for`-loop form of
# benchmarks/mandelbrot_bench_for.jl with a fixed small grid so stdout is
# deterministic.
#
# The hot-loop parameter uses the natural concrete `::ComplexF64` annotation;
# AoT normalizes `cr + ci * im` to the matching concrete Complex type before
# method dispatch. The broadcast form of this gate is tracked by Issues #8789 / #8790.
# The nested-while real-decomposed variant with codegen assertions lives in
# mandelbrot_scalar_aot.jl.
#
# Expected output (julia 1.12.6, 30×20 grid, maxiter=50): 8278

function mandel_point(c::ComplexF64, maxiter::Int64)::Int64
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

result = mandel_count(30, 20, 50)
println(result)
result == 8278
