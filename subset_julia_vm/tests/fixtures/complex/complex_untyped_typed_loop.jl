# Regression: untyped ComplexF64 hot loop must be accepted by the typed-loop recognizer.
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

result = mandel_count(20, 20, 100)
@show result
using Test
@test result == 9180
true
