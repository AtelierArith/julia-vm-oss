import numpy
from setuptools import setup
from Cython.Build import cythonize

setup(
    ext_modules=cythonize(
        [
            "calc_pi.pyx",
            "mandelbrot_for.pyx",
            "mandelbrot_broadcast.pyx",
        ],
        language_level=3,
    ),
    include_dirs=[numpy.get_include()],
)
