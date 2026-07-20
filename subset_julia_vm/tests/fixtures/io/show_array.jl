# Test show methods for arrays

using Test

@testset "show method for arrays" begin
    # Test 1: Vector show with Int64 elements
    v_int = [1, 2, 3]
    buf1 = IOBuffer()
    show(buf1, v_int)
    output1 = String(take!(buf1))
    @test output1 == "[1, 2, 3]"

    # Test 2: Vector show with Float64 elements
    v_float = [1.0, 2.0, 3.0]
    buf2 = IOBuffer()
    show(buf2, v_float)
    output2 = String(take!(buf2))
    @test output2 == "[1.0, 2.0, 3.0]"

    # Test 3: Matrix show with Int64 elements
    m_int = [1 2; 3 4]
    buf3 = IOBuffer()
    show(buf3, m_int)
    output3 = String(take!(buf3))
    @test output3 == "[1 2; 3 4]"

    # Test 4: Matrix show with Float64 elements
    m_float = [1.0 2.0; 3.0 4.0]
    buf4 = IOBuffer()
    show(buf4, m_float)
    output4 = String(take!(buf4))
    @test output4 == "[1.0 2.0; 3.0 4.0]"

    # Test 5: Verify eltype is used (consistency check)
    @test eltype(v_int) == Int64
    @test eltype(v_float) == Float64
    @test eltype(m_int) == Int64
    @test eltype(m_float) == Float64
end

true
