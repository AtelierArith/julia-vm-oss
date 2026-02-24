# Type preservation in tuple broadcast
# Int64 + Int64 → Int64, Float64 + Float64 → Float64

using Test

# Int64 + Int64 → Int64
t1 = (1, 2, 3) .+ (4, 5, 6)
@test t1[1] == 5
@test typeof(t1[1]) == Int64

# Float64 + Float64 → Float64
t2 = (1.0, 2.0) .+ (3.0, 4.0)
@test t2[1] == 4.0
@test typeof(t2[1]) == Float64

# Int64 + Float64 → Float64 (type promotion)
t3 = (1, 2) .+ (1.0, 2.0)
@test t3[1] == 2.0
@test typeof(t3[1]) == Float64

# Int64 * Int64 → Int64
t4 = (2, 3) .* (4, 5)
@test t4[1] == 8
@test typeof(t4[1]) == Int64

# Int64 / Int64 → Float64 (Julia semantics)
t5 = (4, 6) ./ (2, 3)
@test t5[1] == 2.0
@test typeof(t5[1]) == Float64

1.0  # Test passed
