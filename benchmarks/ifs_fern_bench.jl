# IFS fractal (Barnsley fern) benchmark (untyped) — Julia / juliars (AoT) / sjulia 共通ソース
#
# 反復関数系 (Iterated Function System) で Barnsley のシダを生成する。
# 4 つのアフィン変換を確率的に選びながら点列を更新する。乱数は実装非依存に
# するため glibc 互換の LCG を手書きし、全実装で同一の擬似乱数列を使う
# (整数演算 + 浮動小数 + 分岐の混合ワークロード)。checksum は全点の
# (x + y) の総和。
#
# AoT (juliars) 互換のための注意 — いずれも「typed スロットへの代入」での型安定性のため:
#   - `nx`/`ny` は if の前で 0.0 初期化し Float64 に固定する。さらに 3 項和は
#     `(0.85*y + ...) + 1.6` と二項化する。n 項 `+(a,b,c)` の呼び出しは型推論が
#     結果を Any と推論し、Float64 スロットへ代入すると Any→Float64 変換になって
#     AoT 非対応となるため (Issue #6978 / #6968)。
#   - 「分岐内初代入→分岐後参照」のスコープ問題 (Issue #8181) と n 項 `+`/`*` の
#     reachability 問題 (Issue #8180) は修正済み。残る制約は上記 typed スロットの
#     型安定性のみ。
#   - LCG は Int64 内で閉じる: 1103515245 * seed (< 2^61) + 12345, mod 2^31。

function ifs_fern(n)
    seed = 1
    x = 0.0
    y = 0.0
    sx = 0.0
    sy = 0.0
    i = 0
    while i < n
        seed = (1103515245 * seed + 12345) % 2147483648
        r = seed / 2147483648.0
        nx = 0.0
        ny = 0.0
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

function run_one(n)
    t0 = time_ns()
    r = ifs_fern(n)
    t1 = time_ns()
    println("N=", n, " r=", r, " t=", (t1 - t0) / 1.0e9)
end

ifs_fern(1000)          # warmup
run_one(1000000)
run_one(5000000)
run_one(10000000)
