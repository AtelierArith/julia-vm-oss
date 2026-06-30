using Test

@testset "reduction dims String matrix results preserve Julia result types (#4018, #4595)" begin
    A = ["b" "a"; "d" "c"]

    min_cols = minimum(A; dims=1)
    @test typeof(min_cols) === Matrix{String}
    @test eltype(min_cols) === String
    @test size(min_cols) == (1, 2)
    @test min_cols[1, 1] == "b"
    @test min_cols[1, 2] == "a"

    min_rows = minimum(A; dims=2)
    @test typeof(min_rows) === Matrix{String}
    @test eltype(min_rows) === String
    @test size(min_rows) == (2, 1)
    @test min_rows[1, 1] == "a"
    @test min_rows[2, 1] == "c"

    max_cols = maximum(A; dims=1)
    @test typeof(max_cols) === Matrix{String}
    @test eltype(max_cols) === String
    @test size(max_cols) == (1, 2)
    @test max_cols[1, 1] == "d"
    @test max_cols[1, 2] == "c"

    max_rows = maximum(A; dims=2)
    @test typeof(max_rows) === Matrix{String}
    @test eltype(max_rows) === String
    @test size(max_rows) == (2, 1)
    @test max_rows[1, 1] == "b"
    @test max_rows[2, 1] == "d"

    extrema_cols = extrema(A; dims=1)
    @test typeof(extrema_cols) === Matrix{Tuple{String,String}}
    @test eltype(extrema_cols) === Tuple{String,String}
    @test size(extrema_cols) == (1, 2)
    @test extrema_cols[1, 1] == ("b", "d")
    @test extrema_cols[1, 2] == ("a", "c")

    extrema_rows = extrema(A; dims=2)
    @test typeof(extrema_rows) === Matrix{Tuple{String,String}}
    @test eltype(extrema_rows) === Tuple{String,String}
    @test size(extrema_rows) == (2, 1)
    @test extrema_rows[1, 1] == ("a", "b")
    @test extrema_rows[2, 1] == ("c", "d")
end

true
