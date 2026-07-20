# Regression for Issue #9766: top-level Base @assert lowers in CLI/source files.

@assert true

x9766 = 40 + 2
@assert x9766 == 42 "Base @assert with a message should lower from statement position"

true
