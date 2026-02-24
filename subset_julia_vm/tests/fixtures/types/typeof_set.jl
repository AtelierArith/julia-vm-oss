# Test typeof(Set) returns Set{Any} (Issue #527)
# Previously typeof(Set(...)) returned Any instead of Set{Any}

using Test

# Test case 1: typeof empty Set
s1 = Set()
@test typeof(s1) == Set{Any}

# Test case 2: typeof Set with integer elements
s2 = Set([1, 2, 3])
@test typeof(s2) == Set{Any}

# Test case 3: typeof Set with string elements
s3 = Set(["a", "b", "c"])
@test typeof(s3) == Set{Any}

# Return true to indicate success
true
