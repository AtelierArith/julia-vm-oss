using Test

@testset "collect(pairs(...)) preserves Pair eltype (#4018, #4634)" begin
    vec_view = pairs(Int8[5, 6])
    vec_pairs = collect(vec_view)
    @test typeof(vec_pairs) === Vector{Pair{Int64, Int8}}
    @test eltype(vec_pairs) === Pair{Int64, Int8}
    @test vec_pairs[1][1] == 1
    @test vec_pairs[1][2] == Int8(5)
    @test vec_pairs[2][1] == 2
    @test vec_pairs[2][2] == Int8(6)

    tuple_pairs = collect(pairs((Int8(5), Int8(6))))
    @test typeof(tuple_pairs) === Vector{Pair{Int64, Int8}}
    @test eltype(tuple_pairs) === Pair{Int64, Int8}
    @test tuple_pairs[1][1] == 1
    @test tuple_pairs[1][2] == Int8(5)
    @test tuple_pairs[2][1] == 2
    @test tuple_pairs[2][2] == Int8(6)
end

true
