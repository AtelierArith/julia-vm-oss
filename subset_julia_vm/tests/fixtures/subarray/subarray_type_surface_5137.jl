using Test

@testset "SubArray Int8 element type (Issue #5583)" begin
    arr = Int8[4, 5, 6]
    v = view(arr, 1:2)

    expected = SubArray{Int8,1,Vector{Int8},Tuple{UnitRange{Int64}},true}
    @test string(typeof(v)) == "SubArray{Int8, 1, Vector{Int8}, Tuple{UnitRange{Int64}}, true}"
    @test v isa expected
    @test v isa AbstractArray{Int8,1}
    @test v isa AbstractVector{Int8}
    @test eltype(v) == Int8
    @test collect(v) == Int8[4, 5]
end

@testset "SubArray 1D type surface (Issue #5137)" begin
    arr = [1.0, 2.0, 3.0, 4.0, 5.0]
    v = view(arr, 2:4)

    expected = SubArray{Float64,1,Vector{Float64},Tuple{UnitRange{Int64}},true}
    @test string(typeof(v)) == "SubArray{Float64, 1, Vector{Float64}, Tuple{UnitRange{Int64}}, true}"
    @test v isa expected
    @test v isa SubArray
    @test v isa AbstractArray
    @test v isa AbstractArray{Float64,1}
    @test v isa AbstractVector{Float64}
    @test parent(v) === arr
    @test parentindices(v) == (2:4,)

    v[2] = 100.0
    @test arr[3] == 100.0
    @test collect(v) == [2.0, 100.0, 4.0]
end

@testset "SubArray Int64 element type (Issue #5137)" begin
    arr = [4, 5, 6]
    v = view(arr, 1:2)

    expected = SubArray{Int64,1,Vector{Int64},Tuple{UnitRange{Int64}},true}
    @test string(typeof(v)) == "SubArray{Int64, 1, Vector{Int64}, Tuple{UnitRange{Int64}}, true}"
    @test v isa expected
    @test v isa AbstractArray{Int64,1}
    @test v isa AbstractVector{Int64}
    @test eltype(v) == Int64
    @test collect(v) == [4, 5]
end

true
