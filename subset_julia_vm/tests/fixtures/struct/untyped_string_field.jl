# Test untyped struct field containing String
# Issue #361: Cannot access untyped struct field containing String

using Test

struct KeyError2
    key  # Untyped field (implicitly Any)
end

@testset "Untyped struct field containing String (Issue #361)" begin


    k = KeyError2("foo")
    # Access the field and check its length - should work without compile error
    @test (length(k.key)) == 3.0
end

true  # Test passed
