"""IFS fractal (Barnsley fern) benchmark (Python 3.14) — ifs_fern_bench.jl と同一アルゴリズム.

glibc 互換 LCG で擬似乱数を生成し, 4 つのアフィン変換を確率的に適用する.
checksum は全点の (x + y) の総和. uv 経由で実行する:

    uv run --python 3.14 --no-project python benchmarks/ifs_fern_bench.py
"""

import time


def ifs_fern(n):
    seed = 1
    x = 0.0
    y = 0.0
    sx = 0.0
    sy = 0.0
    i = 0
    while i < n:
        seed = (1103515245 * seed + 12345) % 2147483648
        r = seed / 2147483648.0
        if r < 0.01:
            nx = 0.0
            ny = 0.16 * y
        elif r < 0.86:
            nx = 0.85 * x + 0.04 * y
            ny = (-0.04 * x + 0.85 * y) + 1.6
        elif r < 0.93:
            nx = 0.2 * x - 0.26 * y
            ny = (0.23 * x + 0.22 * y) + 1.6
        else:
            nx = -0.15 * x + 0.28 * y
            ny = (0.26 * x + 0.24 * y) + 0.44
        x = nx
        y = ny
        sx = sx + x
        sy = sy + y
        i = i + 1
    return sx + sy


def run_one(n):
    t0 = time.perf_counter_ns()
    r = ifs_fern(n)
    t1 = time.perf_counter_ns()
    print(f"N={n} r={r!r} t={(t1 - t0) / 1.0e9}")


if __name__ == "__main__":
    ifs_fern(1000)  # warmup
    run_one(1000000)
    run_one(5000000)
    run_one(10000000)
