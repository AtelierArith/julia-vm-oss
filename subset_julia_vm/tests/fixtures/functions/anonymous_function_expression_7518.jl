using Test

@testset "anonymous function expressions (Issue #7518)" begin
    f = function (x)
        x + 1
    end
    @test f(2) == 3

    @test (function (x)
        x * 2
    end)(3) == 6

    make_adder(a) = function (x)
        x + a
    end
    add10 = make_adder(10)
    @test add10(2) == 12
end

true
