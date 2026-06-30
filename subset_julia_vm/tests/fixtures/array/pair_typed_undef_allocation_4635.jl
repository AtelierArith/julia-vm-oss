using Test

@testset "Pair typed undef allocation preserves parameters (#4018, #4635)" begin
    a = Array{Pair{Int64, Int8}}(undef, 2)
    @test typeof(a) === Vector{Pair{Int64, Int8}}
    @test eltype(a) === Pair{Int64, Int8}
    a[1] = Pair(1, Int8(2))
    @test a[1][1] == 1
    @test a[1][2] == Int8(2)

    b = similar(Array{Pair{String, Int16}}, (2,))
    @test typeof(b) === Vector{Pair{String, Int16}}
    @test eltype(b) === Pair{String, Int16}
    b[1] = Pair("x", Int16(3))
    @test b[1][1] == "x"
    @test b[1][2] == Int16(3)
end

true
