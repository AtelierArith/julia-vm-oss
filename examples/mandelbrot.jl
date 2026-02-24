# Mandelbrot escape time algorithm
function mandelbrot_escape(c, maxiter)
    z = 0.0 + 0.0im
    for k in 1:maxiter
        if abs2(z) > 4.0        # |z|^2 > 4
            return k
        end
        z = z^2 + c
    end
    return maxiter
end

# Compute grid using broadcast (vectorized)
# xs' creates a row vector, ys is a column vector
# Broadcasting creates a 2D complex matrix C
function mandelbrot_grid(width, height, maxiter)
    xmin = -2.0; xmax = 1.0
    ymin = -1.2; ymax = 1.2

    xs = range(xmin, xmax; length=width)
    ys = range(ymax, ymin; length=height)

    # Create 2D complex grid via broadcasting
    C = xs' .+ im .* ys

    # Apply escape function to all points at once
    # Ref(maxiter) prevents maxiter from being broadcast
    mandelbrot_escape.(C, Ref(maxiter))
end

# ASCII visualization
@time grid = mandelbrot_grid(50, 25, 50)
println("Mandelbrot Set (50x25):")
for row in 1:25
    for col in 1:50
        n = grid[row, col]
        if n == 50
            print("#")
        elseif n > 25
            print("+")
        elseif n > 10
            print(".")
        else
            print(" ")
        end
    end
    println("")
end
