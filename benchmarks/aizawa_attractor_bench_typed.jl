# Aizawa attractor benchmark (typed) — sjulia 専用 (型注釈の効果計測用)
#
# aizawa_attractor_bench.jl と同一アルゴリズム。引数・戻り値・ローカル変数すべてに
# `::Int64` / `::Float64` を付け、VM が I64/F64 専用命令を選べるようにした版。

function aizawa(n::Int64)::Float64
    a::Float64 = 0.95
    b::Float64 = 0.7
    c::Float64 = 0.6
    d::Float64 = 3.5
    e::Float64 = 0.25
    g::Float64 = 0.1
    dt::Float64 = 0.01
    x::Float64 = 0.1
    y::Float64 = 0.0
    z::Float64 = 0.0
    sx::Float64 = 0.0
    sy::Float64 = 0.0
    sz::Float64 = 0.0
    i::Int64 = 0
    while i < n
        dx::Float64 = (z - b) * x - d * y
        dy::Float64 = d * x + (z - b) * y
        dz::Float64 = c + a * z - z * z * z / 3.0 - (x * x + y * y) * (1.0 + e * z) + g * z * x * x * x
        x = x + dx * dt
        y = y + dy * dt
        z = z + dz * dt
        sx = sx + x
        sy = sy + y
        sz = sz + z
        i = i + 1
    end
    sx + sy + sz
end

function run_one(n::Int64)
    t0 = time_ns()
    r = aizawa(n)
    t1 = time_ns()
    println("N=", n, " r=", r, " t=", (t1 - t0) / 1.0e9)
end

aizawa(1000)            # warmup
run_one(1000000)
run_one(5000000)
run_one(10000000)
