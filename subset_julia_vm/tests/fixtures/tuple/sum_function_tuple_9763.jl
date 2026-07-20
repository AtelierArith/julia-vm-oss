using Test

@testset "sum(f, tuple) (Issue #9763)" begin
    @test sum(x -> x * x, (3.0, 4.0)) == 25.0
    @test sum(abs2, (3, 4)) == 25
    @test sum(x -> x + 1, (1, 2, 3)) == 9

    err = nothing
    try
        sum(identity, ())
    catch e
        err = e
    end
    @test err !== nothing

    @test hypot(3.0, 4.0) == 5.0
    @test hypot(3, 4) == 5.0
end

true
