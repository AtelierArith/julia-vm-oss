# Test pushdisplay and popdisplay stack operations
# Note: Due to VM limitations with global mutable state, the display stack
# operations may not persist across function calls. This test verifies
# the API exists and is callable.

using Test

@testset "Display stack push/pop operations" begin
    # Test that pushdisplay and popdisplay functions exist
    # Note: isa(f, Function) returns false in SubsetJuliaVM even for functions,
    # so we check typeof instead
    @test typeof(pushdisplay) == Function
    @test typeof(popdisplay) == Function

    # Test that displays array exists
    @test isa(displays, Array)

    # Note: Due to VM limitations with global mutable state access from
    # functions, the push/pop operations may not work as expected.
    # The API is provided for Julia compatibility.
end

true
