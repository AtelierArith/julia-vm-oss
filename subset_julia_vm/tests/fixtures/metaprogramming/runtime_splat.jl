# Test runtime splat expansion in quote expressions
# :(f($(args...))) where args is determined at runtime

# Basic runtime splat expansion with symbols
args = (:a, :b, :c)
ex = :(f($(args...)))
@assert string(ex) == "f(a, b, c)"

# Single element splat
single = (:x,)
ex2 = :(g($(single...)))
@assert string(ex2) == "g(x)"

# Empty tuple splat
empty = ()
ex3 = :(h($(empty...)))
@assert string(ex3) == "h()"

# Mixed: fixed arguments + splat
args2 = (:b, :c)
ex4 = :(f(a, $(args2...)))
@assert string(ex4) == "f(a, b, c)"

# Splat with numbers
nums = (1, 2, 3)
ex5 = :(add($(nums...)))
@assert string(ex5) == "add(1, 2, 3)"

# Multiple splats in sequence (fixed + splat + fixed + splat)
first_args = (:x,)
second_args = (:y, :z)
ex6 = :(call(a, $(first_args...), b, $(second_args...)))
@assert string(ex6) == "call(a, x, b, y, z)"

# Return true if all tests passed
true
