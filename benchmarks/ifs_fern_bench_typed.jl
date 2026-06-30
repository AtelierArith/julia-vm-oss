# IFS fractal (Barnsley fern) benchmark (typed) — sjulia 専用 (型注釈の効果計測用)
#
# ifs_fern_bench.jl と同一アルゴリズム。引数・戻り値・ローカル変数すべてに
# `::Int64` / `::Float64` を付けた版。

function ifs_fern(n::Int64)::Float64
    seed::Int64 = 1
    x::Float64 = 0.0
    y::Float64 = 0.0
    sx::Float64 = 0.0
    sy::Float64 = 0.0
    i::Int64 = 0
    while i < n
        seed = (1103515245 * seed + 12345) % 2147483648
        r::Float64 = seed / 2147483648.0
        nx::Float64 = 0.0
        ny::Float64 = 0.0
        if r < 0.01
            nx = 0.0
            ny = 0.16 * y
        elseif r < 0.86
            nx = 0.85 * x + 0.04 * y
            ny = (-0.04 * x + 0.85 * y) + 1.6
        elseif r < 0.93
            nx = 0.2 * x - 0.26 * y
            ny = (0.23 * x + 0.22 * y) + 1.6
        else
            nx = -0.15 * x + 0.28 * y
            ny = (0.26 * x + 0.24 * y) + 0.44
        end
        x = nx
        y = ny
        sx = sx + x
        sy = sy + y
        i = i + 1
    end
    sx + sy
end

function run_one(n::Int64)
    t0 = time_ns()
    r = ifs_fern(n)
    t1 = time_ns()
    println("N=", n, " r=", r, " t=", (t1 - t0) / 1.0e9)
end

ifs_fern(1000)          # warmup
run_one(1000000)
run_one(5000000)
run_one(10000000)
