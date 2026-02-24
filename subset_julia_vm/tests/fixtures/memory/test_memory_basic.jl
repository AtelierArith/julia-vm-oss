using Test

@testset "Memory{T} basic operations" begin
    # Construction and length
    m = Memory{Int64}(5)
    @test length(m) == 5
    @test size(m) == (5,)

    # setindex! and getindex
    m[1] = 10
    m[2] = 20
    m[3] = 30
    m[4] = 40
    m[5] = 50
    @test m[1] == 10
    @test m[2] == 20
    @test m[3] == 30
    @test m[4] == 40
    @test m[5] == 50

    # fill!
    m2 = Memory{Int64}(3)
    fill!(m2, 99)
    @test m2[1] == 99
    @test m2[2] == 99
    @test m2[3] == 99

    # copy
    m3 = Memory{Int64}(3)
    m3[1] = 1
    m3[2] = 2
    m3[3] = 3
    m4 = copy(m3)
    @test length(m4) == 3
    @test m4[1] == 1
    @test m4[2] == 2
    @test m4[3] == 3
    # Verify independence (copy is shallow but separate)
    m4[1] = 100
    @test m3[1] == 1
    @test m4[1] == 100

    # Zero-length memory
    m7 = Memory{Int64}(0)
    @test length(m7) == 0
    @test size(m7) == (0,)
end

true
