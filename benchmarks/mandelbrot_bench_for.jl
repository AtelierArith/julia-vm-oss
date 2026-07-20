# Mandelbrot escape-time benchmark (for-loop, Complex) — Julia / juliars (AoT) / sjulia 共通ソース
#
# ADR_BACKEND_STRATEGY.md (Issue #8639) の受け入れカーネル 3/3 のベンチマーク版。
# Mandelbrot は sjulia が **複素数演算** を扱えることを示すゲート:
# ホットループは `z = z * z + c` の ComplexF64 演算そのもので、実数分解しない。
# ループはすべて `for`(ベンチマークスクリプトの正準形)。エスケープ回数の総和を
# checksum として返し、実装間の parity を検証する。
#
# 注: 引数注釈は自然な `::ComplexF64` を使う。AoT は `cr + ci * im` の
# 結果型を同じ具象 Complex 型へ正規化してからメソッド照合する。broadcast 版は mandelbrot_bench_broadcast.jl。
#
# サイズ:
#   1500×1500, maxiter=500 — upstream julia で ~0.5 秒(M系 macOS)。
#     全実装(Julia 本家 / sjulia VM / sjulia AoT / Python 3.14)で比較可能。
# 同一アルゴリズムの Python 版(complex 使用): mandelbrot_bench_for.py

function mandel_point(c::ComplexF64, maxiter::Int64)::Int64
    z = 0.0 + 0.0im
    for k in 1:maxiter
        if abs2(z) > 4.0
            return k - 1
        end
        z = z * z + c
    end
    return maxiter
end

function mandel_count(width::Int64, height::Int64, maxiter::Int64)::Int64
    total = 0
    for y in 1:height
        ci = -1.2 + 2.4 * (y - 1) / (height - 1)
        for x in 1:width
            cr = -2.0 + 3.0 * (x - 1) / (width - 1)
            total += mandel_point(cr + ci * im, maxiter)
        end
    end
    total
end

function run_one(w::Int64, h::Int64, m::Int64)
    t0 = time_ns()
    r = mandel_count(w, h, m)
    t1 = time_ns()
    println(w, "x", h, " maxiter=", m, " total=", r, " t=", (t1 - t0) / 1.0e9)
end

mandel_count(200, 200, 100)   # warmup (JIT / 特殊化)
run_one(1500, 1500, 500)
