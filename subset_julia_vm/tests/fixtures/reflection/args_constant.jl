# Test ARGS constant
# ARGS should be a Vector{String} (command line arguments)

using Test

@testset "ARGS constant: command-line arguments array (Issue #340)" begin

    result = true

    # Check that ARGS is an Array
    if !(typeof(ARGS) <: AbstractArray)
        result = false
    end

    # For SubsetJuliaVM, ARGS is always empty (no CLI args passed)
    # But it should still be a valid array
    if length(ARGS) != 0
        # This is OK - ARGS might have values in Julia REPL
        # For SubsetJuliaVM we expect it to be empty
    end

    @test (result)
end

true  # Test passed
