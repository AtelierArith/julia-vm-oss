# While loop counting

using Test

function main()
    counter = 0
    while counter < 10
        counter = counter + 1
    end
    counter
end

@testset "While loop counting" begin
    @test (main()) == 10.0
end

true  # Test passed
