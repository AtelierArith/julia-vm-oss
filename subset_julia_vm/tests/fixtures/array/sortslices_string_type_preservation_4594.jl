using Test

@testset "sortslices String matrix preserves result eltype (#4018, #4594)" begin
    rows = ["b" "x"; "a" "y"]
    sorted_rows = sortslices(rows; dims=1)
    @test typeof(sorted_rows) === Matrix{String}
    @test eltype(sorted_rows) === String
    @test size(sorted_rows) == (2, 2)
    @test sorted_rows[1, 1] == "a"
    @test sorted_rows[1, 2] == "y"
    @test sorted_rows[2, 1] == "b"
    @test sorted_rows[2, 2] == "x"

    cols = ["b" "a"; "x" "y"]
    sorted_cols = sortslices(cols; dims=2)
    @test typeof(sorted_cols) === Matrix{String}
    @test eltype(sorted_cols) === String
    @test size(sorted_cols) == (2, 2)
    @test sorted_cols[1, 1] == "a"
    @test sorted_cols[1, 2] == "b"
    @test sorted_cols[2, 1] == "y"
    @test sorted_cols[2, 2] == "x"
end

true
