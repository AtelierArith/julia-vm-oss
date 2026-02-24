using Test

# Splat with tuples - passing tuple elements as function arguments
function add3(a, b, c)
    a + b + c
end

function pair(x, y)
    (x, y)
end

@testset "splat with tuples" begin
    t = (10, 20, 30)
    @test add3(t...) == 60

    t2 = (1, 2)
    @test pair(t2...) == (1, 2)

    # Splat part of a tuple combined with literal args
    a = (1, 2)
    @test add3(a..., 3) == 6
    @test add3(0, a...) == 3
end

true
