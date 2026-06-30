using Test

@testset "hcat promotes mixed vector eltypes (#4018, #4655)" begin
    narrow = hcat(Int8[1, 2], Int16[3, 4])
    @test typeof(narrow) === Matrix{Int16}
    @test eltype(narrow) === Int16
    @test size(narrow) == (2, 2)
    @test typeof(narrow[1, 1]) === Int16
    @test narrow[1, 1] == Int16(1)
    @test narrow[1, 2] == Int16(3)
    @test narrow[2, 1] == Int16(2)
    @test narrow[2, 2] == Int16(4)

    floating = hcat(Int8[1, 2], Float32[3, 4])
    @test typeof(floating) === Matrix{Float32}
    @test eltype(floating) === Float32
    @test size(floating) == (2, 2)
    @test typeof(floating[1, 1]) === Float32
    @test floating[1, 1] == Float32(1)
    @test floating[1, 2] == Float32(3)
    @test floating[2, 1] == Float32(2)
    @test floating[2, 2] == Float32(4)

    boxed = hcat(String["a", "b"], Any["c", "d"])
    @test typeof(boxed) === Matrix{Any}
    @test eltype(boxed) === Any
    @test size(boxed) == (2, 2)
    @test boxed[1, 1] == "a"
    @test boxed[1, 2] == "c"
    @test boxed[2, 1] == "b"
    @test boxed[2, 2] == "d"
end

@testset "vcat promotes mixed vector eltypes (#4018, #4655)" begin
    narrow = vcat(Int8[1, 2], Int16[3, 4])
    @test typeof(narrow) === Vector{Int16}
    @test eltype(narrow) === Int16
    @test typeof(narrow[1]) === Int16
    @test narrow[1] == Int16(1)
    @test narrow[2] == Int16(2)
    @test narrow[3] == Int16(3)
    @test narrow[4] == Int16(4)

    floating = vcat(Int8[1, 2], Float32[3, 4])
    @test typeof(floating) === Vector{Float32}
    @test eltype(floating) === Float32
    @test typeof(floating[1]) === Float32
    @test floating[1] == Float32(1)
    @test floating[2] == Float32(2)
    @test floating[3] == Float32(3)
    @test floating[4] == Float32(4)

    boxed = vcat(String["a", "b"], Any["c", "d"])
    @test typeof(boxed) === Vector{Any}
    @test eltype(boxed) === Any
    @test boxed[1] == "a"
    @test boxed[2] == "b"
    @test boxed[3] == "c"
    @test boxed[4] == "d"
end

true
