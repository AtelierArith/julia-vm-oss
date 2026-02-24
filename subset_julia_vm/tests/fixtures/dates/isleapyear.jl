# Test isleapyear function

using Test
using Dates

@testset "isleapyear function for various years" begin
    # 2000 is leap year (divisible by 400)
    # 1900 is not leap year (divisible by 100 but not 400)
    # 2024 is leap year (divisible by 4)
    # 2023 is not leap year
    result = 0.0
    if isleapyear(2000)
        result += 1.0
    end
    if !isleapyear(1900)
        result += 1.0
    end
    if isleapyear(2024)
        result += 1.0
    end
    if !isleapyear(2023)
        result += 1.0
    end
    @test (result) == 4.0
end

true  # Test passed
