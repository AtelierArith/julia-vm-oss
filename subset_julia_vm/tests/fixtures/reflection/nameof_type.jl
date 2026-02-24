# Test nameof function (Issue #493)
# Tests that nameof returns the name of functions and types as Symbol.

# Test 1: nameof for builtin functions
@assert nameof(sin) == :sin
@assert nameof(cos) == :cos
@assert nameof(abs) == :abs

# Test 2: nameof for user-defined functions
f(x) = x + 1
g(x, y) = x * y
@assert nameof(f) == :f
@assert nameof(g) == :g

# Test 3: nameof for primitive types
@assert nameof(Int64) == :Int64
@assert nameof(Float64) == :Float64
@assert nameof(Bool) == :Bool
@assert nameof(String) == :String

# Test 4: nameof for non-parametric collection types
@assert nameof(Array) == :Array
@assert nameof(Tuple) == :Tuple
@assert nameof(Dict) == :Dict

# Test 5: nameof for parametric types (should strip type parameters)
@assert nameof(Vector{Int64}) == :Vector
@assert nameof(Vector{Float64}) == :Vector
@assert nameof(Matrix{Int64}) == :Matrix

# Test 6: nameof with typeof
arr = [1, 2, 3]
@assert nameof(typeof(arr)) == :Vector

# Test 7: nameof for user-defined struct types
struct NameOfTestPoint
    x::Float64
    y::Float64
end
@assert nameof(NameOfTestPoint) == :NameOfTestPoint

true
