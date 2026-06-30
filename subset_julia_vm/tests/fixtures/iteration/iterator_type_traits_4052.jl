using Test

@testset "type-based iterator traits (Issue #4052)" begin
    @test typeof(Base.IteratorSize(typeof(1:5))) === Base.HasShape{1}
    @test typeof(Base.IteratorEltype(typeof(1:5))) === typeof(Base.HasEltype())
    @test typeof(Base.IteratorSize(typeof(1:2:5))) === Base.HasShape{1}
    @test typeof(Base.IteratorEltype(typeof(1:2:5))) === typeof(Base.HasEltype())

    @test typeof(Base.IteratorSize(Vector{Int64})) === Base.HasShape{1}
    @test typeof(Base.IteratorEltype(Vector{Int64})) === typeof(Base.HasEltype())
    @test typeof(Base.IteratorSize(Matrix{Float64})) === Base.HasShape{2}
    @test typeof(Base.IteratorEltype(Matrix{Float64})) === typeof(Base.HasEltype())

    r = 1:5
    r_size = Base.IteratorSize(r)
    @test typeof(r_size) === Base.HasShape{1}
    r_shape = Base._similar_shape(r, r_size)
    @test length(r_shape) == 1
    @test first(r_shape[1]) == 1
    @test last(r_shape[1]) == 5
    r_values = Base._collect(1:1, r, Base.HasEltype(), r_size)
    @test typeof(r_values) === Vector{Int64}
    @test r_values == [1, 2, 3, 4, 5]
end

true
