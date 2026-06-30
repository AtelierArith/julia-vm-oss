using Test

@testset "prod dims preserves upstream reduction result types (#4019, #4614)" begin
    narrow = reshape(Int8[1, 4, 2, 5, 3, 6], 2, 3)

    cprod = prod(narrow; dims=1)
    @test typeof(cprod) == Matrix{Int64}
    @test eltype(cprod) == Int64
    @test typeof(cprod[1]) == Int64
    @test size(cprod) == (1, 3)
    @test cprod[1, 1] == 4
    @test cprod[1, 2] == 10
    @test cprod[1, 3] == 18

    rprod = prod(narrow; dims=2)
    @test typeof(rprod) == Matrix{Int64}
    @test eltype(rprod) == Int64
    @test typeof(rprod[1]) == Int64
    @test size(rprod) == (2, 1)
    @test rprod[1, 1] == 6
    @test rprod[2, 1] == 120

    words = reshape(String["a", "c", "b", "d"], 2, 2)

    word_cols = prod(words; dims=1)
    @test typeof(word_cols) == Matrix{String}
    @test eltype(word_cols) == String
    @test typeof(word_cols[1]) == String
    @test size(word_cols) == (1, 2)
    @test word_cols[1, 1] == "ac"
    @test word_cols[1, 2] == "bd"

    word_rows = prod(words; dims=2)
    @test typeof(word_rows) == Matrix{String}
    @test eltype(word_rows) == String
    @test typeof(word_rows[1]) == String
    @test size(word_rows) == (2, 1)
    @test word_rows[1, 1] == "ab"
    @test word_rows[2, 1] == "cd"

    flags = reshape(Bool[true, true, false, true], 2, 2)
    bool_cols = prod(flags; dims=1)
    @test typeof(bool_cols) == Matrix{Bool}
    @test eltype(bool_cols) == Bool
    @test typeof(bool_cols[1]) == Bool
    @test size(bool_cols) == (1, 2)
    @test bool_cols[1, 1] == true
    @test bool_cols[1, 2] == false
end

true
