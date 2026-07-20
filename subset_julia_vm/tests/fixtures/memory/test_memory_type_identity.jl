using Test

@testset "Memory{T} runtime type identity and reflection" begin
    m = Memory{Int64}(undef, 3)
    @test typeof(m) == Memory{Int64}
    @test eltype(m) == Int64
    @test length(m) == 3

    m[1] = 11
    m[2] = 22
    m[3] = 33
    @test typeof(m[1]) == Int64
    @test m[1] + m[2] + m[3] == 66
    @test typeof(m) == Memory{Int64}
    @test Memory{Int64}.parameters[1] == Int64
    @test m isa Memory{Int64}
    @test !(m isa Memory{Float64})
    @test m isa AbstractVector
    @test m isa AbstractVector{Int64}
    @test m isa AbstractArray
    @test m isa AbstractArray{Int64,1}
    @test !(m isa Vector{Int64})
    @test !(m isa Array{Int64,1})

    mf = Memory{Float64}(undef, 2)
    mf[1] = 1.25
    mf[2] = 2.5
    @test typeof(mf) == Memory{Float64}
    @test eltype(mf) == Float64
    @test typeof(mf[1]) == Float64
    @test mf[1] + mf[2] == 3.75

    mb = Memory{Bool}(undef, 2)
    mb[1] = true
    mb[2] = false
    @test typeof(mb) == Memory{Bool}
    @test eltype(mb) == Bool
    @test typeof(mb[1]) == Bool
    @test mb[1] == true
    @test mb[2] == false
end

true
