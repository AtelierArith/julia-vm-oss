# For loop summing 1 to 10: 1+2+...+10 = 55

using Test

function main()
    total = 0
    for i in 1:10
        total = total + i
    end
    total
end

@testset "For loop summing 1 to 10" begin
    @test (main()) == 55.0
end

true  # Test passed
