# Mandelbrot set benchmark
# Tests complex number arithmetic and iteration

function mandelbrot_iter(cr::Float64, ci::Float64, max_iter::Int64)::Int64
    zr = 0.0
    zi = 0.0
    i = 0
    while i < max_iter
        zr2 = zr * zr
        zi2 = zi * zi
        if zr2 + zi2 > 4.0
            return i
        end
        zi = 2.0 * zr * zi + ci
        zr = zr2 - zi2 + cr
        i = i + 1
    end
    max_iter
end

function mandelbrot(width::Int64, height::Int64, max_iter::Int64)::Int64
    x_min = -2.0
    x_max = 1.0
    y_min = -1.5
    y_max = 1.5

    dx = (x_max - x_min) / Float64(width)
    dy = (y_max - y_min) / Float64(height)

    total = 0
    y = 0
    while y < height
        x = 0
        while x < width
            cr = x_min + Float64(x) * dx
            ci = y_min + Float64(y) * dy
            total = total + mandelbrot_iter(cr, ci, max_iter)
            x = x + 1
        end
        y = y + 1
    end
    total
end

# Benchmark entry point
function main()
    width = 200
    height = 200
    max_iter = 100
    result = mandelbrot(width, height, max_iter)
    println(result)
end

main()
