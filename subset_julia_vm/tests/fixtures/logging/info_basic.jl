# Test @info macro - basic usage
# This test verifies that the @info macro outputs correctly.
# Since we can't capture stdout in fixture tests, we just verify it runs.

using Test

@testset "@info basic" begin
    # Basic message
    @info "Test message"

    # With variable
    x = 42
    @info "Value is $x"

    @test true  # If we get here, macros expanded and ran correctly
end

true
