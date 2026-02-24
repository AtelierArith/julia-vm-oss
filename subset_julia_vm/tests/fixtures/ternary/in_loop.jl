# Ternary inside a for loop

using Test

function main()
    total = 0.0
    for i in 1:10
        total = total + (i > 5 ? i : 0)
    end
    total  # 6 + 7 + 8 + 9 + 10 = 40
end

@testset "Ternary inside a for loop" begin
    @test (main()) == 40.0
end

true  # Test passed
