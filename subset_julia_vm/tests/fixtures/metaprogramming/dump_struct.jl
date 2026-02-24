# Test dump with struct
# This tests the improved dump function that uses getfield and isdefined

struct Point
    x::Int64
    y::Int64
end

# Create a Point instance
p = Point(3, 4)

# Test getfield - get field by index (1-based)
f1 = getfield(p, 1)  # Should be 3
f2 = getfield(p, 2)  # Should be 4

# Test isdefined - check if field exists
d1 = isdefined(p, 1)  # Should be true
d2 = isdefined(p, 2)  # Should be true
d3 = isdefined(p, 3)  # Should be false (out of bounds)

# Verify the results
result = f1 == 3 && f2 == 4 && d1 && d2 && !d3

# Return 1.0 if all tests pass, 0.0 otherwise
Float64(result)
