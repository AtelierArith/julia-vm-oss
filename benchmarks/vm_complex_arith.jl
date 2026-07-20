# Complex{Float64} arithmetic VM benchmark (Issue #9125).
#
# Exercises the hot Complex arithmetic path: in a typed function each
# `z = z*z + c` iteration runs the Julia Complex `*`/`+` methods via direct
# Calls, cloning the operand StructInstances on every slot load and return.
# Before Issue #9125 each clone cost 2 heap allocations (struct_name String +
# field Vec); with `struct_name: Rc<str>` it costs 1.
#
# The loop structure keeps only a single live Complex value per iteration so
# allocation pressure is dominated by the per-operation result cost.

function mandelbrot_complex(cr::Float64, ci::Float64, maxiter::Int64)::Int64
    c = Complex{Float64}(cr, ci)
    z = Complex{Float64}(0.0, 0.0)
    iter = 0
    while iter < maxiter && real(z) * real(z) + imag(z) * imag(z) <= 4.0
        z = z * z + c
        iter = iter + 1
    end
    iter
end

function mandelbrot_complex_count(width::Int64, height::Int64, maxiter::Int64)::Int64
    total = 0
    y = 1
    while y <= height
        ci = (2.0 * Float64(y) / Float64(height)) - 1.0
        x = 1
        while x <= width
            cr = (3.5 * Float64(x) / Float64(width)) - 2.5
            total = total + mandelbrot_complex(cr, ci, maxiter)
            x = x + 1
        end
        y = y + 1
    end
    total
end

function main()
    # Deliberately small: Complex arithmetic on the VM is ~20x slower per
    # inner iteration than the scalar-float mandelbrot (each op runs the
    # Julia Complex method with StructInstance clones), so this size keeps a
    # single run around a second for criterion A/B loops.
    result = mandelbrot_complex_count(30, 20, 25)
    println(result)
end

main()
