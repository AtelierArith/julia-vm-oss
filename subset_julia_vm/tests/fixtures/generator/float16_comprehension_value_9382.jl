# Issue #9382: an inline `Float16(x)` constructor call in a comprehension body
# produced Float64 element *values* even though the container tag was already
# Vector{Float16} (fixed by #9301 / PR #9368). The single-iterator comprehension
# body push matched F16 into the default `_` arm which coerced the body to F64;
# F16 now rides the same value-preserving boxed push path as F32.

# The issue's MWE: element value type must be Float16, not Float64.
c = [Float16(i) for i in 1:3]
println(typeof(c))
println(typeof(c[1]))
println(c == Float16[1, 2, 3])

# collect over the equivalent generator: container tag AND element values.
g = collect(Float16(i) for i in 1:3)
println(typeof(g))
println(typeof(g[1]))
println(g == Float16[1, 2, 3])

# A Float16-typed body expression (not just a bare constructor call) also
# keeps Float16 values.
x = [Float16(i) + Float16(1) for i in 1:2]
println(typeof(x), " ", typeof(x[1]), " ", x == Float16[2, 3])

# Filtered comprehension takes the same single-iterator push path.
v = [Float16(i) for i in 1:4 if i % 2 == 0]
println(typeof(v), " ", typeof(v[1]), " ", v == Float16[2, 4])

# Float32 / Float64 analogues were already correct and must stay correct
# (F64 keeps its dedicated typed push path).
y = [Float32(i) for i in 1:3]
z = [Float64(i) for i in 1:3]
println(typeof(y[1]), " ", typeof(z[1]))

all_ok =
    typeof(c) === Vector{Float16} &&
    typeof(c[1]) === Float16 &&
    c == Float16[1, 2, 3] &&
    typeof(g) === Vector{Float16} &&
    typeof(g[1]) === Float16 &&
    g == Float16[1, 2, 3] &&
    typeof(x) === Vector{Float16} &&
    typeof(x[1]) === Float16 &&
    x == Float16[2, 3] &&
    typeof(v) === Vector{Float16} &&
    typeof(v[1]) === Float16 &&
    v == Float16[2, 4] &&
    typeof(y[1]) === Float32 &&
    typeof(z[1]) === Float64

println(all_ok)
all_ok
