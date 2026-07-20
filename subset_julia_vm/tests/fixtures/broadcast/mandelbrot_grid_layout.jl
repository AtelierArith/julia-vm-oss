using Test

# Regression coverage for the Mandelbrot heatmap sample's grid *layout*.
#
# The `intermediate/mandelbrot_heatmap.jl` sample once built its escape-time
# grid by filling a flat `Vector` in ROW-major order (outer loop over rows /
# height, inner loop over columns / width) and then calling
# `reshape(data, height, width)`. But Julia's `reshape` is COLUMN-major, so the
# fill order and the reshape order disagreed and the grid came out scrambled —
# rendering as vertical stripes in the heatmap. Upstream Julia produces the same
# scramble, so this was a bug in the sample code, not in the VM or the renderer.
#
# The correct construction (now shipped) builds the grid with a 2D "outer"
# broadcast `xs' .+ im .* ys`, which yields a `(height, width)` matrix directly.
# For `heatmap(x, y, grid)`, `grid[j, i]` must be the escape value at
# `(x[i], y[j])`. This fixture pins that orientation and documents the trap.

mandelbrot_escape(c, maxiter) = begin
    z = 0.0 + 0.0im
    for k in 1:maxiter
        abs2(z) > 4.0 && return k
        z = z^2 + c
    end
    maxiter
end

xrange(width)  = range(-2.0, 1.0; length=width)
yrange(height) = range(-1.2, 1.2; length=height)

# Shipped construction: 2D outer broadcast → (height, width) grid.
function mandelbrot_grid(width, height, maxiter)
    xs = xrange(width)
    ys = yrange(height)
    C = xs' .+ im .* ys
    mandelbrot_escape.(C, Ref(maxiter))
end

# Explicit 2D reference: grid[j, i] = escape at (x_i, y_j).
function reference_grid(width, height, maxiter)
    xs = xrange(width); ys = yrange(height)
    G = Matrix{Int}(undef, height, width)
    for j in 1:height, i in 1:width
        G[j, i] = mandelbrot_escape(xs[i] + ys[j] * im, maxiter)
    end
    G
end

# Row-major fill of a flat vector: data[(j-1)*width + i] = escape at (x_i, y_j).
function rowmajor_data(width, height, maxiter)
    xs = xrange(width); ys = yrange(height)
    data = Vector{Int}(undef, height * width)
    k = 1
    for j in 1:height, i in 1:width
        data[k] = mandelbrot_escape(xs[i] + ys[j] * im, maxiter)
        k += 1
    end
    data
end

@testset "mandelbrot grid: broadcast layout matches 2D reference" begin
    w, h, m = 6, 4, 9
    g = mandelbrot_grid(w, h, m)
    @test size(g) == (h, w)
    @test g == reference_grid(w, h, m)
    # heatmap orientation: grid[j, i] is the value at (x_i, y_j).
    xs = xrange(w); ys = yrange(h)
    @test g[1, 1] == mandelbrot_escape(xs[1] + ys[1] * im, m)
    @test g[h, w] == mandelbrot_escape(xs[w] + ys[h] * im, m)
end

@testset "mandelbrot grid: row-major fill + reshape IS scrambled (the trap)" begin
    w, h, m = 6, 4, 9
    ref = reference_grid(w, h, m)
    data = rowmajor_data(w, h, m)
    # The old sample: reshape(data, height, width). reshape is column-major, so
    # a row-major fill scrambles into vertical stripes — this must NOT match.
    @test reshape(data, h, w) != ref
    # Correct recovery for a row-major fill: reshape to (width, height), transpose.
    @test permutedims(reshape(data, w, h)) == ref
end

true
