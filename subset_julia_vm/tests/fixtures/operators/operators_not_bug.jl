# Test negation of function return values
# Regression test for: "Type error: expected I64, got Bool" bug

using Test

function issubset_proper(a, b)
    return issubset(a, b) && !issubset(b, a)
end

function issuperset(a, b)
    return issubset(b, a)
end

function issuperset_proper(a, b)
    return issubset_proper(b, a)
end

function returns_false()
    return false
end

function returns_true()
    return true
end

function wrapper()
    return returns_false()
end

@testset "negation of function return values (regression test for Bool type error)" begin

    # Helper functions for proper subset/superset (not in Julia Base)



    # Basic wrapper function test



    # Test basic negation
    @assert !false
    @assert !(1 == 2)

    # Test negation of direct function calls
    @assert !returns_false()
    @assert !(returns_true() == false)

    # Test negation of wrapper functions (this was the failing case)
    @assert !wrapper()
    @assert !(!returns_true())

    # Test with set operators (original bug case)
    a = [1.0, 2.0, 3.0]
    b = [1.0, 2.0, 3.0, 4.0, 5.0]

    # These should all work (previously some failed with "Type error: expected I64, got Bool")
    @assert !issubset(b, a)
    @assert !(a ⊇ b)
    @assert !issuperset(a, b)
    @assert !(issuperset(a, b))
    @assert !issuperset_proper(a, b)

    # Additional edge cases
    @assert !(1 > 2)
    @assert !(!true)
    @assert !(!(1 == 1))

    @test (true)
end

true  # Test passed
