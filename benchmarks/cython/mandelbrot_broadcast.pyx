# cython: language_level=3
# cython: boundscheck=False
# cython: wraparound=False
# cython: cdivision=True
#
# NOTE: This is a hand-optimized Cython benchmark.  It does NOT use Python's
# complex type or C99 double complex; instead it splits the recurrence
# z = z*z + c into explicit real/imaginary double variables (zr, zi).  This
# decomposition is a manual optimization and is NOT a fair language-level
# comparison against Julia/sjulia sources that write the loop with ComplexF64.

import numpy as np
cimport numpy as np


cdef inline long _mandel_point(double cr, double ci, long maxiter):
    cdef double zr = 0.0
    cdef double zi = 0.0
    cdef double zr2, zi2
    cdef long k
    for k in range(1, maxiter + 1):
        zr2 = zr * zr
        zi2 = zi * zi
        if zr2 + zi2 > 4.0:
            return k - 1
        zi = 2.0 * zr * zi + ci
        zr = zr2 - zi2 + cr
    return maxiter


def mandelbrot_grid(long width, long height, long maxiter):
    cdef np.ndarray[np.double_t, ndim=2] counts
    cdef np.ndarray[np.double_t, ndim=1] xs = np.linspace(-2.0, 1.0, width)
    cdef np.ndarray[np.double_t, ndim=1] ys = np.linspace(1.2, -1.2, height)
    cdef long x, y
    cdef double cr, ci

    counts = np.zeros((height, width), dtype=np.double)
    for y in range(height):
        ci = ys[y]
        for x in range(width):
            cr = xs[x]
            counts[y, x] = _mandel_point(cr, ci, maxiter)
    return counts.sum()
