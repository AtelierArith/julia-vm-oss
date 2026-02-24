# Julia Manual: Modules
# https://docs.julialang.org/en/v1/manual/modules/
# Tests module definition, export, and qualified access.

using Test

module MathUtils
    export double, triple

    double(x) = 2x
    triple(x) = 3x

    # Non-exported helper
    quadruple(x) = 4x
end

using .MathUtils

@testset "Module exports" begin
    @test double(5) == 10
    @test triple(5) == 15
end

@testset "Qualified access" begin
    @test MathUtils.quadruple(5) == 20
    @test MathUtils.double(3) == 6
end

module Counter
    export make_counter

    function make_counter()
        count = 0
        function inc()
            count += 1
            count
        end
        inc
    end
end

using .Counter

@testset "Module with closures" begin
    c = make_counter()
    @test c() == 1
    @test c() == 2
    @test c() == 3
end

true
