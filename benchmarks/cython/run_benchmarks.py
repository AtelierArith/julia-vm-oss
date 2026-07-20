import time
import calc_pi
import mandelbrot_for
import mandelbrot_broadcast


def benchmark_calc_pi(N, warmup=True):
    if warmup:
        calc_pi.calc_pi(1000)
    t0 = time.perf_counter()
    result = calc_pi.calc_pi(N)
    t1 = time.perf_counter()
    print(f"coprime π  N={N}: π ≈ {result}  t={t1 - t0:.3f}s")


def benchmark_mandelbrot_for(w=1500, h=1500, maxiter=500, warmup=True):
    if warmup:
        mandelbrot_for.mandel_count(200, 200, 100)
    t0 = time.perf_counter()
    r = mandelbrot_for.mandel_count(w, h, maxiter)
    t1 = time.perf_counter()
    print(f"Mandelbrot for-loop {w}x{h} maxiter={maxiter} total={r}  t={t1 - t0:.3f}s")


def benchmark_mandelbrot_broadcast(w=1700, h=1360, maxiter=500, warmup=True):
    if warmup:
        mandelbrot_broadcast.mandelbrot_grid(50, 40, 50)
    t0 = time.perf_counter()
    r = mandelbrot_broadcast.mandelbrot_grid(w, h, maxiter)
    t1 = time.perf_counter()
    print(f"Mandelbrot broadcast {w}x{h} maxiter={maxiter} total={r}  t={t1 - t0:.3f}s")


if __name__ == "__main__":
    benchmark_calc_pi(5000)
    benchmark_calc_pi(10000)
    benchmark_mandelbrot_for()
    benchmark_mandelbrot_broadcast()
