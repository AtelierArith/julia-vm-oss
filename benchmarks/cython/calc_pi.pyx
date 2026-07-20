# cython: language_level=3
# cython: boundscheck=False
# cython: wraparound=False
# cython: cdivision=True

from libc.math cimport sqrt


cdef inline long _mygcd(long a, long b):
    cdef long tmp
    while b != 0:
        tmp = b
        b = a % b
        a = tmp
    return a


def calc_pi(long N):
    cdef long cnt = 0
    cdef long a, b
    for a in range(1, N + 1):
        for b in range(1, N + 1):
            if _mygcd(a, b) == 1:
                cnt += 1
    return sqrt(6.0 / (<double>cnt / (N * N)))
