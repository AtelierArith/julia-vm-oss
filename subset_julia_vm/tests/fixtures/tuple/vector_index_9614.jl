using Test

t_9614 = (10, "b", 3.5, :d)

@testset "Tuple indexing with vector indices (Issue #9614)" begin
    @test t_9614[[1, 3]] === (10, 3.5)
    @test typeof(t_9614[[1, 3]]) === Tuple{Int64, Float64}

    @test t_9614[Int32[2, 4]] === ("b", :d)
    @test typeof(t_9614[Int32[2, 4]]) === Tuple{String, Symbol}

    @test t_9614[UInt8[1, 3]] === (10, 3.5)
    @test t_9614[Float64[1.0, 3.0]] === (10, 3.5)
    @test t_9614[Any[1, 2.0]] === (10, "b")

    @test t_9614[Bool[true, false, true, false]] === (10, 3.5)
    @test t_9614[Int[]] === ()
    @test typeof(t_9614[Int[]]) === Tuple{}

    @test t_9614[true] === 10
    @test_throws BoundsError t_9614[Bool[true, false]]
    @test_throws BoundsError t_9614[[0]]
    @test_throws BoundsError t_9614[[5]]
end

true
