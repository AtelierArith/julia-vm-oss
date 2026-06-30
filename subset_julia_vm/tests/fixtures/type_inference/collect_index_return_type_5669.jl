# A function returning an element indexed from a `collect(...)` result must not
# coerce the element type (Issue #5669). Previously `collect(arr)` over an
# untyped parameter inferred `Array{Float64}`, so the function return coerced a
# genuine Int64 to Float64 (and even errored for String elements).

# Int element: must return Int64, not Float64
function f(arr)
    x = collect(arr)
    x[2]
end
r1 = (f([5, 3, 1, 4, 2]) === 3)

# Float element: stays Float64
r2 = (f([1.5, 2.5, 3.5]) === 2.5)

# String element: must return the String, not error / coerce to numeric
r3 = (f(["x", "y", "z"]) == "y")

# collect(arr)[i] inline in return position
function g(arr)
    collect(arr)[1]
end
r4 = (g([10, 20, 30]) === 10)

# partialsort over an integer array returns Int64 (base/sort.jl uses collect)
r5 = (partialsort([5, 3, 1, 4, 2], 2) === 2)

# partialsort over a String array returns the String
r6 = (partialsort(["c", "a", "b"], 2) == "b")

# Typed parameter still preserves the element type
function h(arr::Vector{Int})
    x = collect(arr)
    x[2]
end
r7 = (h([5, 3, 1, 4, 2]) === 3)

# collect of a range inside a function stays Int64
function cr(n)
    x = collect(1:n)
    x[2]
end
r8 = (cr(5) === 2)

r1 && r2 && r3 && r4 && r5 && r6 && r7 && r8
