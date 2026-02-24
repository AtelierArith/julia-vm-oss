# Test NamedTuple typeof, isa, keys, values, pairs (Issue #412)
# Tests that NamedTuple type operations work correctly.

# Test 1: typeof returns proper NamedTuple type
nt = (a=1, b=2)
t = typeof(nt)
@assert isequal(string(t), "@NamedTuple{a::Int64, b::Int64}")

# Test 2: isa(nt, NamedTuple) works correctly
@assert isa(nt, NamedTuple) == true

# Test 3: keys returns tuple of symbols
ks = keys(nt)
@assert length(ks) == 2
@assert ks[1] == :a
@assert ks[2] == :b

# Test 4: values returns tuple of values
vs = values(nt)
@assert length(vs) == 2
@assert vs[1] == 1
@assert vs[2] == 2

# Test 5: pairs returns tuple of (symbol, value) pairs
ps = pairs(nt)
@assert length(ps) == 2
@assert ps[1] == (:a, 1)
@assert ps[2] == (:b, 2)

# Test 6: Nested NamedTuple types
nt2 = (outer=(inner=1,),)
t2 = typeof(nt2)
@assert isa(nt2, NamedTuple) == true
@assert isa(nt2.outer, NamedTuple) == true

# Test 7: NamedTuple with mixed types
nt3 = (x=42, y=3.14, z="hello")
@assert isa(nt3, NamedTuple) == true
@assert nt3.x == 42
@assert nt3.y == 3.14
@assert isequal(nt3.z, "hello")

true
