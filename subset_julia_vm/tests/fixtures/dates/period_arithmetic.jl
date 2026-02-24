# Test Period arithmetic

using Test
using Dates

@testset "Period arithmetic (+, -, *)" begin
    d1 = Day(10)
    d2 = Day(5)
    sum_days = d1 + d2
    diff_days = d1 - d2
    prod_days = d1 * 3
    Float64(value(sum_days) * 100 + value(diff_days) * 10 + value(prod_days))
end

true  # Test passed
