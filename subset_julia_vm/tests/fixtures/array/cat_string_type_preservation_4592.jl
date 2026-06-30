using Test

@testset "cat String vectors preserves result eltype (#4018, #4592)" begin
    v = cat(["a", "b"], ["c"]; dims=1)
    @test typeof(v) === Vector{String}
    @test eltype(v) === String
    @test v == ["a", "b", "c"]
end

@testset "cat String matrices preserves result eltype (#4018, #4592)" begin
    A = ["a" "b"]
    B = ["c" "d"]

    vertical = cat(A, B; dims=1)
    @test typeof(vertical) === Matrix{String}
    @test eltype(vertical) === String
    @test size(vertical) == (2, 2)
    @test vertical[1, 1] == "a"
    @test vertical[1, 2] == "b"
    @test vertical[2, 1] == "c"
    @test vertical[2, 2] == "d"

    horizontal = cat(A, B; dims=2)
    @test typeof(horizontal) === Matrix{String}
    @test eltype(horizontal) === String
    @test size(horizontal) == (1, 4)
    @test horizontal[1, 1] == "a"
    @test horizontal[1, 2] == "b"
    @test horizontal[1, 3] == "c"
    @test horizontal[1, 4] == "d"
end

@testset "cat mixed eltypes promotes result eltype (#4018, #4651)" begin
    narrow = cat(Int8[1], Int16[2]; dims=1)
    @test typeof(narrow) === Vector{Int16}
    @test eltype(narrow) === Int16
    @test narrow == Int16[1, 2]

    floating = cat(Int8[1], Float32[2]; dims=1)
    @test typeof(floating) === Vector{Float32}
    @test eltype(floating) === Float32
    @test floating == Float32[1, 2]

    boxed = cat(String["a"], Any["b"]; dims=1)
    @test typeof(boxed) === Vector{Any}
    @test eltype(boxed) === Any
    @test boxed == Any["a", "b"]

    bool_int = cat(Bool[true], Int8[2]; dims=1)
    @test typeof(bool_int) === Vector{Int8}
    @test eltype(bool_int) === Int8
    @test bool_int == Int8[1, 2]
end

true
