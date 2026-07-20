# Mandelbrot escape-time benchmark (broadcast) — Julia / sjulia 共通ソース
#
# mandelbrot_bench_for.jl のスカラー for ループ版に対する broadcast 版。
# `xs' .+ im .* ys` で複素グリッドを作り、エスケープ関数を `.` broadcast で
# 全点に適用する — sjulia の broadcast + Complex サポートを示すベンチマーク。
# checksum(エスケープ回数の総和)は同一パラメータなら for 版と同じ定義。
#
# サイズ:
#   1700×1360, maxiter=500 — upstream julia で ~0.5 秒(M系 macOS)。
#     Julia 本家 / sjulia VM / Python 3.14(numpy) で比較可能。
#
# AoT (juliars): #8790 解消後にビルド可能。`--minimal-prelude` で
#   生成 Rust が rustc を通るようになったが、checksum が Julia 本家と
#   数カウントずれる（broadcast 実装の丸め経路の違い、Issue #9659 と同系）。
#   タイミング比較には影響しない。
# Python 版(numpy 使用): mandelbrot_bench_broadcast.py

function mandelbrot_escape(c::ComplexF64, maxiter::Int64)::Int64
    z = 0.0 + 0.0im
    for k in 1:maxiter
        if abs2(z) > 4.0
            return k - 1
        end
        z = z * z + c
    end
    return maxiter
end

function mandelbrot_grid(width::Int64, height::Int64, maxiter::Int64)
    xs = range(-2.0, 1.0; length=width)
    ys = range(1.2, -1.2; length=height)
    C = xs' .+ im .* ys
    counts = mandelbrot_escape.(C, maxiter)
    sum(counts)
end

function run_one(w::Int64, h::Int64, m::Int64)
    t0 = time_ns()
    r = mandelbrot_grid(w, h, m)
    t1 = time_ns()
    println(w, "x", h, " maxiter=", m, " total=", r, " t=", (t1 - t0) / 1.0e9)
end

mandelbrot_grid(50, 40, 50)   # warmup (JIT / 特殊化)
run_one(1700, 1360, 500)
