using Test

@testset "Memory{T} with different types" begin
    # Memory{Float64}
    mf = Memory{Float64}(3)
    mf[1] = 1.5
    mf[2] = 2.5
    mf[3] = 3.5
    @test mf[1] == 1.5
    @test mf[2] == 2.5
    @test mf[3] == 3.5

    # fill! with Float64
    fill!(mf, 0.0)
    @test mf[1] == 0.0
    @test mf[2] == 0.0
    @test mf[3] == 0.0

    # Memory{String}
    ms = Memory{String}(2)
    ms[1] = "hello"
    ms[2] = "world"
    @test ms[1] == "hello"
    @test ms[2] == "world"

    # Copy preserves values
    ms2 = copy(ms)
    @test ms2[1] == "hello"
    @test ms2[2] == "world"
    @test length(ms2) == 2

    # Memory{Bool}
    mb = Memory{Bool}(4)
    fill!(mb, true)
    @test mb[1] == true
    @test mb[4] == true
    mb[2] = false
    @test mb[2] == false
end

true
