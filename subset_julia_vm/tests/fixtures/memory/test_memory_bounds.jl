using Test

# Helper for testing setindex! bounds errors
function memory_setindex_test!(m, v, i)
    m[i] = v
end

@testset "Memory{T} bounds checking" begin
    m = Memory{Int64}(3)
    m[1] = 10
    m[2] = 20
    m[3] = 30

    # Valid access
    @test m[1] == 10
    @test m[3] == 30

    # Out of bounds getindex
    @test_throws BoundsError m[0]
    @test_throws BoundsError m[4]

    # Out of bounds setindex! (via helper function)
    @test_throws BoundsError memory_setindex_test!(m, 99, 0)
    @test_throws BoundsError memory_setindex_test!(m, 99, 4)

    # Empty memory — all access is out of bounds
    m2 = Memory{Int64}(0)
    @test_throws BoundsError m2[1]
end

true
