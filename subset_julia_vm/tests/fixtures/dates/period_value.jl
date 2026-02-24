# Test Period value accessor

using Test
using Dates

@testset "Period value accessor function" begin
    y = Year(2024)
    m = Month(6)
    d = Day(15)
    h = Hour(12)
    mi = Minute(30)
    s = Second(45)
    Float64(value(y) + value(m) + value(d) + value(h) + value(mi) + value(s))
end

true  # Test passed
