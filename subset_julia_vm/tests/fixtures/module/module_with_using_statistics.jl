using Test
using .My

module My
using Statistics

export f

function f(x)
    mean(x)
end

end

@testset "Module with using Statistics statement" begin


    @test (My.f([1,2,3])) == 2.0
end

true  # Test passed
