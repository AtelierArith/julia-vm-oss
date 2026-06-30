# Aizawa attractor benchmark (untyped, for-loop) — Julia / juliars (AoT) / sjulia 共通ソース
#
# `aizawa_attractor_bench.jl` と同一アルゴリズム。while ループを for ループに変更。

function aizawa(n)
    a = 0.95; b = 0.7; c = 0.6; d = 3.5; e = 0.25; g = 0.1
    dt = 0.01
    x = 0.1; y = 0.0; z = 0.0
    sx = 0.0; sy = 0.0; sz = 0.0
    for i in 0:n-1
        dx = (z - b) * x - d * y
        dy = d * x + (z - b) * y
        dz = c + a * z - z * z * z / 3.0 - (x * x + y * y) * (1.0 + e * z) + g * z * x * x * x
        x = x + dx * dt
        y = y + dy * dt
        z = z + dz * dt
        sx = sx + x; sy = sy + y; sz = sz + z
    end
    sx + sy + sz
end

function run_one(n)
    t0 = time_ns()
    r = aizawa(n)
    t1 = time_ns()
    println("N=", n, " r=", r, " t=", (t1 - t0) / 1.0e9)
end

aizawa(1000)            # warmup (JIT / 特殊化)
run_one(1000000)
run_one(5000000)
run_one(10000000)
