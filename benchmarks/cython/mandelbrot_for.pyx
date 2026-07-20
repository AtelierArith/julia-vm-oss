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


def mandel_count(long width, long height, long maxiter):
    cdef long total = 0
    cdef long x, y
    cdef double cr, ci
    for y in range(1, height + 1):
        ci = -1.2 + 2.4 * (y - 1) / (height - 1)
        for x in range(1, width + 1):
            cr = -2.0 + 3.0 * (x - 1) / (width - 1)
            total += _mandel_point(cr, ci, maxiter)
    return total
