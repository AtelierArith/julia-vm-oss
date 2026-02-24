using Test

# Test preprocess and broadcast_unalias (Issue #2543)

@testset "broadcast_unalias" begin
    # Different objects: no aliasing, return src unchanged
    a = [1, 2, 3]
    b = [4, 5, 6]
    result = broadcast_unalias(a, b)
    @test result === b

    # Same object: aliasing detected
    result2 = broadcast_unalias(a, a)
    @test result2 !== a  # Should be a copy
    @test result2 == a   # But with same values

    # Nothing destination: no aliasing
    result3 = broadcast_unalias(nothing, a)
    @test result3 === a
end

@testset "preprocess" begin
    # preprocess wraps arrays in Extruded
    A = [1, 2, 3]
    preprocessed = preprocess(nothing, A)
    @test isa(preprocessed, Extruded)
    @test preprocessed.x === A

    # preprocess on Broadcasted: recursively processes bc_args
    bc = Broadcasted(nothing, +, ([1, 2, 3], [4, 5, 6]), (1:3,))
    bc_prep = preprocess(nothing, bc)
    @test isa(bc_prep, Broadcasted)
    @test isa(bc_prep.bc_args[1], Extruded)
    @test isa(bc_prep.bc_args[2], Extruded)

    # preprocess on scalar: extrude returns scalar as-is
    @test preprocess(nothing, 42) == 42
end

true
