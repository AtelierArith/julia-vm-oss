using Test

@testset "prod! preserves typed in-place reduction semantics (#4019, #4616)" begin
    words = reshape(String["a", "c", "b", "d"], 2, 2)

    word_cols = similar(words, 1, 2)
    returned_cols = prod!(word_cols, words)
    @test returned_cols === word_cols
    @test typeof(word_cols) == Matrix{String}
    @test eltype(word_cols) == String
    @test word_cols[1, 1] == "ac"
    @test word_cols[1, 2] == "bd"

    word_rows = Vector{String}(undef, 2)
    returned_rows = prod!(word_rows, words)
    @test returned_rows === word_rows
    @test typeof(word_rows) == Vector{String}
    @test eltype(word_rows) == String
    @test word_rows[1] == "ab"
    @test word_rows[2] == "cd"

    flags = reshape(Bool[true, true, false, true], 2, 2)
    bool_cols = similar(flags, 1, 2)
    prod!(bool_cols, flags)
    @test typeof(bool_cols) == Matrix{Bool}
    @test eltype(bool_cols) == Bool
    @test bool_cols[1, 1] == true
    @test bool_cols[1, 2] == false

    narrow = reshape(Int8[1, 4, 2, 5], 2, 2)
    int_cols = zeros(Int64, 1, 2)
    prod!(int_cols, narrow)
    @test typeof(int_cols) == Matrix{Int64}
    @test eltype(int_cols) == Int64
    @test int_cols[1, 1] == 4
    @test int_cols[1, 2] == 10
end

true
