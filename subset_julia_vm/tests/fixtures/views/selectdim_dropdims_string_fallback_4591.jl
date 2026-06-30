using Test

@testset "selectdim String matrix fallback preserves eltype (#4018, #4591)" begin
    A = ["a" "b"; "c" "d"]

    row = selectdim(A, 1, 1)
    @test eltype(row) === String
    @test collect(row) == ["a", "b"]

    col = selectdim(A, 2, 2)
    @test eltype(col) === String
    @test collect(col) == ["b", "d"]
end

@testset "dropdims String matrix fallback preserves eltype (#4018, #4591)" begin
    row_matrix = ["a" "b"]
    row = dropdims(row_matrix, dims=1)
    @test typeof(row) === Vector{String}
    @test eltype(row) === String
    @test row == ["a", "b"]

    col_matrix = reshape(["a", "b"], 2, 1)
    col = dropdims(col_matrix, dims=2)
    @test typeof(col) === Vector{String}
    @test eltype(col) === String
    @test col == ["a", "b"]
end

true
