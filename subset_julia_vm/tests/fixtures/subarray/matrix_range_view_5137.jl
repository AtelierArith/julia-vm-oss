using Test

@testset "Matrix range SubArray view (Issue #5137)" begin
    A = reshape(collect(1:9), 3, 3)
    v = view(A, 1:2, 2:3)

    expected = SubArray{Int64,2,Matrix{Int64},Tuple{UnitRange{Int64},UnitRange{Int64}},false}
    @test string(typeof(v)) == "SubArray{Int64, 2, Matrix{Int64}, Tuple{UnitRange{Int64}, UnitRange{Int64}}, false}"
    @test v isa expected
    @test v isa AbstractArray{Int64,2}
    @test v isa AbstractMatrix{Int64}
    @test size(v) == (2, 2)
    @test size(v, 3) == 1
    @test length(v) == 4
    @test ndims(v) == 2
    @test parent(v) === A
    @test parentindices(v) == (1:2, 2:3)
    @test v[2, 1] == 5

    v[1, 2] = 99
    @test A[1, 3] == 99

    A[2, 2] = 55
    @test v[2, 1] == 55

    c = collect(v)
    @test c == [4 99; 55 8]
    @test typeof(c) == Matrix{Int64}
    @test size(c) == (2, 2)
    c[1, 1] = -1
    @test A[1, 2] == 4
end

true
