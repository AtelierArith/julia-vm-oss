function build_grid(width::Int64, height::Int64)
    xs = collect(1.0:Float64(width))
    ys = collect(1.0:Float64(height))
    return xs' .+ im .* ys
end

grid = build_grid(200, 150)
println(sum(grid))
