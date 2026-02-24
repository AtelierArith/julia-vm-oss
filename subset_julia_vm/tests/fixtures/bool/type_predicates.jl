# Test Bool type bounds and predicates (Issue #472, #604)
# Based on Julia's base/bool.jl:8-9, 157-158

using Test

@testset "Bool type bounds and predicates (typemin/typemax/iszero/isone) (Issue #472, #604)" begin

    result = 0.0

    # Test typemin(Bool) = false
    if typemin(Bool) == false
        result = result + 1.0
    end

    # Test typemax(Bool) = true
    if typemax(Bool) == true
        result = result + 1.0
    end

    # Test iszero(x::Bool) - Issue #604 fix
    # iszero(false) should be true, iszero(true) should be false
    if iszero(false) == true
        result = result + 1.0
    end
    if iszero(true) == false
        result = result + 1.0
    end

    # Test isone(x::Bool) - Issue #604 fix
    # isone(true) should be true, isone(false) should be false
    if isone(true) == true
        result = result + 1.0
    end
    if isone(false) == false
        result = result + 1.0
    end

    @test (result) == 6.0
end

true  # Test passed
