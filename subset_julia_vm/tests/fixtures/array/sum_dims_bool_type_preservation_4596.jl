using Test

@testset "sum Bool matrix dims returns Int64 result (#4018, #4596)" begin
    A = [true false; true true]

    by_column = sum(A; dims=1)
    @test typeof(by_column) === Matrix{Int64}
    @test eltype(by_column) === Int64
    @test size(by_column) == (1, 2)
    @test by_column[1, 1] == 2
    @test by_column[1, 2] == 1

    by_row = sum(A; dims=2)
    @test typeof(by_row) === Matrix{Int64}
    @test eltype(by_row) === Int64
    @test size(by_row) == (2, 1)
    @test by_row[1, 1] == 1
    @test by_row[2, 1] == 2
end

true
