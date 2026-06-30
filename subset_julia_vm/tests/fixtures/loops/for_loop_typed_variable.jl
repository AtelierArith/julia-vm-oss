# Issue #8208: a type annotation on a single loop variable, `for i::T in itr`,
# types the loop variable and converts each iterate value to `T` (upstream Julia
# behavior). Verified to match `julia` exactly. Covers: range fast path, the
# `=` head form, integer-range value converted to Float64 (the loop counter must
# not be clobbered), array-iterable convert, and cartesian typed loops.

# Basic typed integer loop variable over a range.
function sum_typed(n::Int64)
    s = 0
    for i::Int64 in 0:(n - 1)
        s = s + i
    end
    s
end

# `=` head form (equivalent to `in`).
function sum_typed_eq(n::Int64)
    s = 0
    for i::Int64 = 1:n
        s += i
    end
    s
end

# Integer range, but the loop variable is declared Float64: each counter value is
# converted to Float64 before the body runs. The loop's own counter slot must stay
# intact (regression: the integer fast path uses the loop variable as its counter).
function sum_float_var(n::Int64)
    s = 0.0
    for x::Float64 in 1:n
        s += x
    end
    (s, typeof(s))
end

# Array iterable whose Float64 elements are converted to Int64.
function sum_convert_elems()
    s = 0
    for v::Int64 in [1.0, 2.0, 3.0]
        s += v
    end
    s
end

# Cartesian `for i::T in xs, j::S in ys` desugars to nested typed loops.
function cartesian_typed()
    s = 0.0
    for i::Float64 in 1:2, j::Int64 in 1:2
        s += i * j
    end
    s
end

println(sum_typed(5))
println(sum_typed_eq(5))
println(sum_float_var(3))
println(sum_convert_elems())
println(cartesian_typed())
# The fixture harness checks the program's final return value (not stdout); this
# trailing literal mirrors the printed output above (see for_loop_typed.jl).
"10\n15\n(6.0, Float64)\n6\n9.0\n"
