# Test @timed macro - returns NamedTuple with value and time fields
# Julia's @timed returns (value=..., time=..., bytes=..., gctime=..., ...)
# SubsetJuliaVM returns simplified (value=..., time=...)

# Basic usage with computation
t = @timed begin
    x = 0
    for i in 1:100
        x = x + i
    end
    x
end

# Result should be the computed value (access via .value)
@assert t.value == 5050

# Elapsed time should be a Float64 (access via .time)
@assert typeof(t.time) == Float64

# Time should be non-negative
@assert t.time >= 0.0

# Simple expression
t2 = @timed 2 + 3
@assert t2.value == 5
@assert t2.time >= 0.0

# With function call
add(a, b) = a + b
t3 = @timed add(10, 20)
@assert t3.value == 30
@assert t3.time >= 0.0

true
