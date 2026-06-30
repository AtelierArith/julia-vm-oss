# Test typeof(Set) returns upstream-compatible Set element types (Issues #527, #4018)
# Previously typeof(Set(...)) returned Any instead of a concrete Set type

using Test

# Test case 1: typeof empty Set
s1 = Set()
@test typeof(s1) == Set{Any}

# Test case 2: typeof Set with integer elements
s2 = Set([1, 2, 3])
@test typeof(s2) == Set{Int64}

# Test case 3: typeof Set with string elements
s3 = Set(["a", "b", "c"])
@test typeof(s3) == Set{String}

# Return true to indicate success
true
