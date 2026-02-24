# Test oneto function - convenience constructor for OneTo
# oneto(n) is equivalent to OneTo(n)

using Test

@testset "oneto function (Issue #490)" begin
    # oneto(n) creates OneTo(n)
    r = oneto(3)

    # Test iteration
    total = 0
    count = 0
    for x in r
        total = total + x
        count = count + 1
    end
    @test (total == 6)   # 1 + 2 + 3 = 6
    @test (count == 3)   # 3 elements
end

true  # Test passed
