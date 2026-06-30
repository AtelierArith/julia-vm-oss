# Mandelbrot escape-time (broadcast + Complex + @time)

function mandelbrot_escape(c, maxiter)
    z = 0.0 + 0.0im
    for k in 1:maxiter
        if abs2(z) > 4.0
            return k
        end
        z = z^2 + c
    end
    return maxiter
end

function mandelbrot_grid(width, height, maxiter)
    xmin = -2.0
    xmax = 1.0
    ymin = -1.2
    ymax = 1.2

    xs = range(xmin, xmax; length=width)
    ys = range(ymax, ymin; length=height)

    # Create 2D complex grid via broadcasting
    c = xs' .+ im .* ys

    # Ref(maxiter) keeps maxiter scalar in broadcast
    mandelbrot_escape.(c, Ref(maxiter))
end

function main()
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
end

main()
