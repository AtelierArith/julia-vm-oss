using Test

# Test Extruded struct, newindexer, and newindex (Issue #2537)

@testset "Extruded and newindexer" begin
    # Test newindexer on a 1D array
    A = [10, 20, 30]
    keeps_A, defaults_A = newindexer(A)
    @test keeps_A == (true,)
    @test defaults_A == (1,)

    # Test newindexer on a scalar-like 1-element array
    B = [42]
    keeps_B, defaults_B = newindexer(B)
    @test keeps_B == (false,)
    @test defaults_B == (1,)

    # Test Extruded construction
    ext = Extruded(A, keeps_A, defaults_A)
    @test ext.x === A
    @test ext.keeps == (true,)
    @test ext.defaults == (1,)

    # Test extrude function
    ext2 = extrude(A)
    @test isa(ext2, Extruded)
    @test ext2.keeps == (true,)

    # Test extrude on scalar (should return scalar as-is)
    @test extrude(5) == 5
    @test extrude(3.14) == 3.14

    # Test newindex with integer index
    # When keep is true, pass through the index
    @test newindex(3, (true,), (1,)) == 3
    # When keep is false, use the default
    @test newindex(3, (false,), (1,)) == 1
end

true
