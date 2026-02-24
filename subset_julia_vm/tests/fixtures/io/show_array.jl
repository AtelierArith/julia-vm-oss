# Test show methods for arrays using eltype
# Tests that show(io, arr) displays the correct element type

using Test

@testset "show method for arrays using eltype" begin
    # Test 1: Vector show with Int64 elements
    v_int = [1, 2, 3]
    buf1 = IOBuffer()
    show(buf1, v_int)
    output1 = take!(buf1)
    @test occursin("Vector{Int64}", output1)
    @test occursin("3-element", output1)

    # Test 2: Vector show with Float64 elements
    v_float = [1.0, 2.0, 3.0]
    buf2 = IOBuffer()
    show(buf2, v_float)
    output2 = take!(buf2)
    @test occursin("Vector{Float64}", output2)
    @test occursin("3-element", output2)

    # Test 3: Matrix show with Int64 elements
    m_int = [1 2; 3 4]
    buf3 = IOBuffer()
    show(buf3, m_int)
    output3 = take!(buf3)
    @test occursin("Matrix{Int64}", output3)
    @test occursin("2×2", output3)

    # Test 4: Matrix show with Float64 elements
    m_float = [1.0 2.0; 3.0 4.0]
    buf4 = IOBuffer()
    show(buf4, m_float)
    output4 = take!(buf4)
    @test occursin("Matrix{Float64}", output4)
    @test occursin("2×2", output4)

    # Test 5: Verify eltype is used (consistency check)
    @test eltype(v_int) == Int64
    @test eltype(v_float) == Float64
    @test eltype(m_int) == Int64
    @test eltype(m_float) == Float64
end

true
