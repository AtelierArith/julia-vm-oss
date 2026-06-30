"""Aizawa attractor benchmark (Python 3.14, for-loop).

`aizawa_attractor_bench.py` と同一アルゴリズム。while ループを for ループに変更。
"""

import time


def aizawa(n):
    a = 0.95; b = 0.7; c = 0.6; d = 3.5; e = 0.25; g = 0.1
    dt = 0.01
    x = 0.1; y = 0.0; z = 0.0
    sx = 0.0; sy = 0.0; sz = 0.0
    for i in range(n):
        dx = (z - b) * x - d * y
        dy = d * x + (z - b) * y
        dz = c + a * z - z * z * z / 3.0 - (x * x + y * y) * (1.0 + e * z) + g * z * x * x * x
        x = x + dx * dt
        y = y + dy * dt
        z = z + dz * dt
        sx = sx + x; sy = sy + y; sz = sz + z
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
