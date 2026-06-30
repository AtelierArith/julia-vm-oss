using Test

@testset "mixed Vector set operations allocate promoted result eltype (#4018, #4630)" begin
    u = union(Int8[1, 2], Int16[2, 3])
    @test u == Int16[1, 2, 3]
    @test typeof(u) === Vector{Int16}
    @test eltype(u) === Int16

    i = intersect(Int8[1, 2], Int16[2, 3])
    @test i == Int16[2]
    @test typeof(i) === Vector{Int16}
    @test eltype(i) === Int16

    d = setdiff(Int8[1, 2], Int16[2, 3])
    @test d == Int16[1]
    @test typeof(d) === Vector{Int16}
    @test eltype(d) === Int16

    s = symdiff(Int8[1, 2], Int16[2, 3])
    @test s == Int16[1, 3]
    @test typeof(s) === Vector{Int16}
    @test eltype(s) === Int16

    f = union(Int8[1, 2], Float32[2, 3])
    @test f == Float32[1, 2, 3]
    @test typeof(f) === Vector{Float32}
    @test eltype(f) === Float32
end

true
