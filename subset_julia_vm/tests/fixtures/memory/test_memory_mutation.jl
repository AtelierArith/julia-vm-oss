using Test

@testset "Memory{T} mutation patterns" begin
    # In-place update via loop
    m = Memory{Int64}(5)
    for i in 1:5
        m[i] = i * 10
    end
    @test m[1] == 10
    @test m[3] == 30
    @test m[5] == 50

    # Element swap
    m2 = Memory{Int64}(3)
    m2[1] = 1
    m2[2] = 2
    m2[3] = 3
    tmp = m2[1]
    m2[1] = m2[3]
    m2[3] = tmp
    @test m2[1] == 3
    @test m2[2] == 2
    @test m2[3] == 1

    # Overwrite all elements with fill!
    m3 = Memory{Float64}(4)
    m3[1] = 1.0
    m3[2] = 2.0
    m3[3] = 3.0
    m3[4] = 4.0
    fill!(m3, -1.0)
    @test m3[1] == -1.0
    @test m3[2] == -1.0
    @test m3[3] == -1.0
    @test m3[4] == -1.0

    # Accumulation via indexing
    m4 = Memory{Int64}(5)
    fill!(m4, 0)
    for i in 1:5
        m4[i] = m4[i] + i
    end
    @test m4[1] == 1
    @test m4[2] == 2
    @test m4[5] == 5
end

true
