using Test

@testset "HOF filter Bool predicate specialization (Issue #5094)" begin
    xs = [-3, 0, 1, 4]
    @test filter(iszero, xs) == [0]
    @test typeof(filter(iszero, xs)) == Vector{Int64}
    @test filter(isone, xs) == [1]
    @test typeof(filter(isone, xs)) == Vector{Int64}
    @test filter(signbit, xs) == [-3]
    @test typeof(filter(signbit, xs)) == Vector{Int64}

    i32s = Int32[-3, 0, 1, 4]
    @test filter(iszero, i32s) == Int32[0]
    @test typeof(filter(iszero, i32s)) == Vector{Int32}
    @test filter(isone, i32s) == Int32[1]
    @test typeof(filter(isone, i32s)) == Vector{Int32}
    @test filter(signbit, i32s) == Int32[-3]
    @test typeof(filter(signbit, i32s)) == Vector{Int32}

    u32s = UInt32[3, 0, 1, 4]
    @test filter(iszero, u32s) == UInt32[0]
    @test typeof(filter(iszero, u32s)) == Vector{UInt32}
    @test filter(isone, u32s) == UInt32[1]
    @test typeof(filter(isone, u32s)) == Vector{UInt32}
    @test filter(signbit, u32s) == UInt32[]
    @test typeof(filter(signbit, u32s)) == Vector{UInt32}

    fs = Float64[-1.5, 0.0, 1.0, 2.5]
    @test filter(iszero, fs) == [0.0]
    @test typeof(filter(iszero, fs)) == Vector{Float64}
    @test filter(isone, fs) == [1.0]
    @test typeof(filter(isone, fs)) == Vector{Float64}
    @test filter(signbit, fs) == [-1.5]
    @test typeof(filter(signbit, fs)) == Vector{Float64}

    bs = [true, false, true]
    @test filter(iszero, bs) == Bool[false]
    @test typeof(filter(iszero, bs)) == Vector{Bool}
    @test filter(isone, bs) == Bool[true, true]
    @test typeof(filter(isone, bs)) == Vector{Bool}
    @test filter(signbit, bs) == Bool[]
    @test typeof(filter(signbit, bs)) == Vector{Bool}
end

true
