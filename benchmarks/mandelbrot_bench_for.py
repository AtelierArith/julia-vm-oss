# Mandelbrot escape-time benchmark (for-loop, complex) — Python mirror of
# mandelbrot_bench_for.jl. Uses Python's built-in complex type in the hot loop
# (z = z * z + c, no real/imag decomposition) so the comparison is
# interpreter-vs-interpreter on the same complex-arithmetic algorithm.
# The numpy/broadcast variant lives in mandelbrot_bench_broadcast.py.
#
# Run: uv run --python 3.14 --no-project benchmarks/mandelbrot_bench_for.py

import time


def mandel_point(c: complex, maxiter: int) -> int:
    z = 0.0 + 0.0j
    for k in range(1, maxiter + 1):
        if z.real * z.real + z.imag * z.imag > 4.0:
            return k - 1
        z = z * z + c
    return maxiter


def mandel_count(width: int, height: int, maxiter: int) -> int:
    total = 0
    for y in range(1, height + 1):
        ci = -1.2 + 2.4 * (y - 1) / (height - 1)
        for x in range(1, width + 1):
            cr = -2.0 + 3.0 * (x - 1) / (width - 1)
            total += mandel_point(complex(cr, ci), maxiter)
    return total


def run_one(w: int, h: int, m: int) -> None:
    t0 = time.perf_counter_ns()
    r = mandel_count(w, h, m)
    t1 = time.perf_counter_ns()
    print(f"{w}x{h} maxiter={m} total={r} t={(t1 - t0) / 1.0e9}")


mandel_count(200, 200, 100)  # warmup
run_one(1500, 1500, 500)
