using Test

@testset "mapslices String matrix allocates typed slices and Int result (#4018, #4593)" begin
    A = ["a" "bb" "ccc"; "dddd" "eeeee" "ffffff"]

    by_column = mapslices(length, A; dims=1)
    @test typeof(by_column) === Matrix{Int64}
    @test eltype(by_column) === Int64
    @test size(by_column) == (1, 3)
    @test by_column[1, 1] == 2
    @test by_column[1, 2] == 2
    @test by_column[1, 3] == 2

    by_row = mapslices(length, A; dims=2)
    @test typeof(by_row) === Matrix{Int64}
    @test eltype(by_row) === Int64
    @test size(by_row) == (2, 1)
    @test by_row[1, 1] == 3
    @test by_row[2, 1] == 3
end

true
