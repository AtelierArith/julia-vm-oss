using Test

double(x) = x * 2

@testset "generator expression collect preserves matrix shape (Issue #4127)" begin
    source = [1 2; 3 4]

    direct = collect(Base.Generator(double, source))
    @test typeof(direct) === Matrix{Int64}
    @test eltype(direct) === Int64
    @test size(direct) == (2, 2)
    @test direct == [2 4; 6 8]

    generated = collect(double(x) for x in source)
    @test typeof(generated) === Matrix{Int64}
    @test eltype(generated) === Int64
    @test size(generated) == (2, 2)
    @test generated == [2 4; 6 8]
end

true
