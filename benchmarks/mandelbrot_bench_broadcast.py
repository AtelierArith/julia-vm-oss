# Mandelbrot escape-time benchmark (broadcast, numpy) — Python mirror of
# mandelbrot_bench_broadcast.jl. Builds the complex grid by broadcasting
# (xs[None, :] + 1j * ys[:, None]) and iterates the escape map on whole
# arrays with an active mask — numpy's idiomatic vectorized form of the same
# algorithm (numpy use is explicitly allowed for this variant).
#
# The escape count matches the scalar definition: the number of z-updates
# performed before |z|^2 first exceeds 4, capped at maxiter.
#
# Run: uv run --python 3.14 --no-project --with numpy benchmarks/mandelbrot_bench_broadcast.py

import time

import numpy as np


def mandelbrot_grid(width: int, height: int, maxiter: int) -> int:
    # Match Julia broadcast grid exactly: xs' .+ im .* ys where
    # xs = range(-2.0, 1.0; length=width) and ys = range(1.2, -1.2; length=height).
    # np.linspace endpoints differ slightly, so use the same explicit formulas.
    xs = -2.0 + 3.0 * np.arange(width, dtype=np.float64) / (width - 1)
    ys = 1.2 - 2.4 * np.arange(height, dtype=np.float64) / (height - 1)
    C = xs[None, :] + 1j * ys[:, None]

    z = np.zeros_like(C)
    counts = np.zeros(C.shape, dtype=np.int64)
    active = np.ones(C.shape, dtype=bool)
    for _ in range(maxiter):
        # Check escape before updating, matching Julia semantics:
        # Julia returns k-1 where k is the first iteration with |z|^2 > 4.
        active &= (z.real * z.real + z.imag * z.imag) <= 4.0
        if not active.any():
            break
        z[active] = z[active] * z[active] + C[active]
        counts[active] += 1
    return int(counts.sum())


def run_one(w: int, h: int, m: int) -> None:
    t0 = time.perf_counter_ns()
    r = mandelbrot_grid(w, h, m)
    t1 = time.perf_counter_ns()
    print(f"{w}x{h} maxiter={m} total={r} t={(t1 - t0) / 1.0e9}")


mandelbrot_grid(50, 40, 50)  # warmup
run_one(1700, 1360, 500)
