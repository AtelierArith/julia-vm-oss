# Aizawa attractor benchmark (untyped) — Julia / juliars (AoT) / sjulia 共通ソース
#
# 3 次元カオス力学系 (Aizawa attractor) を陽的 Euler 法で数値積分する。
# ホットループは純 Float64 のスカラー演算のみ。checksum として全ステップの
# (x + y + z) の総和を返し、最適化による dead-code 除去を防ぐと同時に
# 各実装間の結果一致 (parity) を検証できるようにする。
#
# 注: 末尾の 3 項和 `sx + sy + sz`（n 項 `+(a,b,c)`）は以前 juliars(AoT) が
# afoldl/HasShape を経由できず非対応だったが Issue #8180 で修正済み。自然な形で書ける。

function aizawa(n)
    a = 0.95; b = 0.7; c = 0.6; d = 3.5; e = 0.25; g = 0.1
    dt = 0.01
    x = 0.1; y = 0.0; z = 0.0
    sx = 0.0; sy = 0.0; sz = 0.0
    i = 0
    while i < n
        dx = (z - b) * x - d * y
        dy = d * x + (z - b) * y
        dz = c + a * z - z * z * z / 3.0 - (x * x + y * y) * (1.0 + e * z) + g * z * x * x * x
        x = x + dx * dt
        y = y + dy * dt
        z = z + dz * dt
        sx = sx + x; sy = sy + y; sz = sz + z
        i = i + 1
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
