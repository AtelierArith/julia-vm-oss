using Test

# Issue #10968: a parametric constructor call whose TYPE argument is a local
# runtime `DataType` value (`make(T, x) = R{T}(x)`, `T` an ordinary parameter)
# must still select and run the matching braced inner-constructor method
# (not perform raw field construction that skips every inner-constructor
# body while still stamping the right nominal type parameter).

struct RuntimeOverload10968{T}
    x::T
    RuntimeOverload10968{T}(x::Int) where T = new(T(101))
    RuntimeOverload10968{T}(x::String) where T = new(T(202))
end

make_overload_10968(T, x) = RuntimeOverload10968{T}(x)

a = make_overload_10968(Int64, 1)
b = make_overload_10968(Int64, "s")
@test a.x == 101
@test b.x == 202
@test typeof(a) === RuntimeOverload10968{Int64}
@test typeof(b) === RuntimeOverload10968{Int64}

# The runtime type argument genuinely varies across calls (megamorphic call
# site), not just a single monomorphic shape.
types10968 = Any[Int64, Float64, Int32]
results10968 = Any[]
for T in types10968
    push!(results10968, make_overload_10968(T, 1).x)
    push!(results10968, make_overload_10968(T, "s").x)
end
@test results10968 == Any[101, 202, 101.0, 202.0, Int32(101), Int32(202)]

# Calling through a `Function` value (no static call-site specialization)
# takes the same runtime-dispatch path and must still run the constructor
# bodies.
f10968 = make_overload_10968
@test f10968(Int64, 1).x == 101
@test f10968(Int64, "s").x == 202

true
