# Mandelbrot scalar AoT fixture.
#
# Exercises typed Float64/Int64 arithmetic, nested while loops, and early
# return inside a loop — without Complex numbers or broadcasting so the AoT
# codegen path does not require unsupported Base expansions.
#
# Expected output: 9054
# (sum of iteration counts over a 30×20 grid, maxiter=50)

function mandel_point(cr::Float64, ci::Float64, maxiter::Int64)::Int64
    zr = 0.0
    zi = 0.0
    iter = 0
    while iter < maxiter
        r2 = zr * zr
        i2 = zi * zi
        if r2 + i2 > 4.0
            return iter
        end
        zi = 2.0 * zr * zi + ci
        zr = r2 - i2 + cr
        iter = iter + 1
    end
    return iter
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

result = mandel_count(30, 20, 50)
println(result)
result == 9054
