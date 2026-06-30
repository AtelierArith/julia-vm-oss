using Test

@testset "Memory{T} constructor uses where type parameter value" begin
    make_memory(::Type{T}, n) where T = Memory{T}(n)

    m = make_memory(Int64, 2)
    @test typeof(m) == Memory{Int64}
    @test eltype(m) == Int64
    @test length(m) == 2
    m[1] = 11
    @test m[1] == 11
    @test typeof(m[1]) == Int64

    mf = make_memory(Float64, 3)
    @test typeof(mf) == Memory{Float64}
    @test eltype(mf) == Float64
    @test length(mf) == 3
    mf[2] = 1.5
    @test mf[2] == 1.5
    @test typeof(mf[2]) == Float64
end

@testset "Type{S} arguments bind S as a value" begin
    identity_type(::Type{S}) where S = S
    select_type(mem, ::Type{S}, n) where S = S

    @test identity_type(Float64) == Float64
    @test select_type(Memory{Int64}(2), Float64, 3) == Float64
end

@testset "GenericMemory similar uses runtime type parameter" begin
    make_similar(mem, ::Type{S}, n) where S = similar(mem, S, n)

    sm = make_similar(Memory{Int64}(2), Float64, 3)
    @test typeof(sm) == Memory{Float64}
    @test eltype(sm) == Float64
    @test length(sm) == 3
    sm[1] = 2.5
    @test sm[1] == 2.5

    sm_same = similar(Memory{Int64}(4))
    @test typeof(sm_same) == Memory{Int64}
    @test eltype(sm_same) == Int64
    @test length(sm_same) == 4

    sm_len = similar(Memory{Int64}(4), 2)
    @test typeof(sm_len) == Memory{Int64}
    @test length(sm_len) == 2

    sm_tuple = similar(Memory{Int64}(4), Float64, (2,))
    @test typeof(sm_tuple) == Memory{Float64}
    @test eltype(sm_tuple) == Float64
    @test length(sm_tuple) == 2

    arr = similar(Memory{Int64}(4), Float64, (2, 2))
    @test size(arr) == (2, 2)
    @test length(arr) == 4
    arr[2, 2] = 3.5
    @test arr[2, 2] == 3.5
    @test typeof(arr[2, 2]) == Float64
end

true
