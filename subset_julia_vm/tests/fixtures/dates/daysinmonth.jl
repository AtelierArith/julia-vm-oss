# Test daysinmonth function

using Test
using Dates

@testset "daysinmonth for various months including leap year February" begin
    # January has 31 days
    # February has 28 days in non-leap year, 29 in leap year
    # April has 30 days
    jan = daysinmonth(2023, 1)
    feb_non_leap = daysinmonth(2023, 2)
    feb_leap = daysinmonth(2024, 2)
    apr = daysinmonth(2023, 4)
    @test (Float64(jan + feb_non_leap + feb_leap + apr)) == 118.0
end

true  # Test passed
