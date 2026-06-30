# Issue #7800: an inline range literal with an integer start and a *non-literal*
# float step (e.g. `0:(2π/12):2π`, where `2π/12` is a computed expression, not a
# float literal) in a top-level for-head used to iterate zero times. The #3551
# fix only diverted float *literals*; a Float-typed expression stayed on the I64
# fast path, which truncated the step to 0. Counts/sums below must match upstream
# Julia (13 iterations, sum ≈ 40.84070449666731).

# int start, non-literal float step, inline top-level for-head
a = 0
for u in 0:(2π/12):2π
    global a += 1
end

# float start, float step, inline top-level for-head
b = 0
for u in 0.0:0.5:6.0
    global b += 1
end

# int start, literal float step
c = 0
for u in 0:0.5:6.0
    global c += 1
end

# loop variable must be the real float value, not a truncated Int
s = 0.0
for u in 0:(2π/12):2π
    global s += u
end

# function-scope case with a local accumulator
function count_int_start_float_step()
    n = 0
    for u in 0:(2π/12):2π
        n += 1
    end
    return n
end

a == 13 &&
    b == 13 &&
    c == 13 &&
    abs(s - 40.84070449666731) < 1.0e-9 &&
    count_int_start_float_step() == 13 &&
    length(collect(0:(2π/12):2π)) == 13
