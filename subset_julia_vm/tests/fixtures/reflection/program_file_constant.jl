# Test PROGRAM_FILE constant
# PROGRAM_FILE should be a String (path to the running script)

using Test

@testset "PROGRAM_FILE constant: path to running script (Issue #340)" begin

    result = true

    # Check that PROGRAM_FILE is a String
    if !(typeof(PROGRAM_FILE) <: AbstractString)
        result = false
    end

    # For SubsetJuliaVM in embedded mode, PROGRAM_FILE is empty string
    # For Julia running a script, it would contain the script path
    # We just check that it's a valid String (either empty or with content)

    @test (result)
end

true  # Test passed
