# Aizawa attractor benchmark (N=500M) — Julia / juliars (AoT) / sjulia 共通ソース
# 3 次元カオス力学系を陽的 Euler 法で数値積分し、checksum として全ステップの
# (x + y + z) 総和を返す。

using Printf

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

result = aizawa(500_000_000)
@printf "%.17g\n" result
