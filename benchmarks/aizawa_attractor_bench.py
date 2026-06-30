"""Aizawa attractor benchmark (Python 3.14) — aizawa_attractor_bench.jl と同一アルゴリズム.

陽的 Euler 法による 3 次元カオス力学系の数値積分. checksum は全ステップの
(x + y + z) の総和. uv 経由で実行する:

    uv run --python 3.14 --no-project python benchmarks/aizawa_attractor_bench.py
"""

import time


def aizawa(n):
    a = 0.95; b = 0.7; c = 0.6; d = 3.5; e = 0.25; g = 0.1
    dt = 0.01
    x = 0.1; y = 0.0; z = 0.0
    sx = 0.0; sy = 0.0; sz = 0.0
    i = 0
    while i < n:
        dx = (z - b) * x - d * y
        dy = d * x + (z - b) * y
        dz = c + a * z - z * z * z / 3.0 - (x * x + y * y) * (1.0 + e * z) + g * z * x * x * x
        x = x + dx * dt
        y = y + dy * dt
        z = z + dz * dt
        sx = sx + x; sy = sy + y; sz = sz + z
        i = i + 1
    return (sx + sy) + sz


def run_one(n):
    t0 = time.perf_counter_ns()
    r = aizawa(n)
    t1 = time.perf_counter_ns()
    print(f"N={n} r={r!r} t={(t1 - t0) / 1.0e9}")


if __name__ == "__main__":
    aizawa(1000)  # warmup
    run_one(1000000)
    run_one(5000000)
    run_one(10000000)
